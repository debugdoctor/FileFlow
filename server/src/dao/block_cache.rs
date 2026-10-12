use super::db::FileBlock;
use std::{
    collections::HashMap,
    env,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

#[derive(Default)]
struct Transfer {
    bytes: u64,
    next_start: u64,
    acknowledged: HashMap<u64, u64>,
    done: bool,
}

struct Entry {
    transfer: Transfer,
    expires: Instant,
}

#[derive(Default)]
struct State {
    blocks: HashMap<(String, u64), FileBlock>,
    transfers: HashMap<String, Entry>,
    bytes: u64,
}

impl State {
    fn cleanup(&mut self, now: Instant) {
        self.transfers.retain(|_, entry| entry.expires > now);
        self.blocks
            .retain(|(id, _), _| self.transfers.contains_key(id));
        self.bytes = self
            .transfers
            .values()
            .map(|entry| entry.transfer.bytes)
            .sum();
    }
}

/// Bounded RAM cache. Acknowledgements release bytes, never disk-spool them.
pub struct BlockCache {
    state: Arc<RwLock<State>>,
    pub max_bytes: u64,
    pub window_bytes: u64,
    ttl: Duration,
}

fn env_limit(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

impl BlockCache {
    pub fn from_env() -> Self {
        let max_bytes = env_limit("MAX_CACHE_BYTES", 256 * 1024 * 1024).max(1024 * 1024);
        let window_bytes = env_limit("TRANSFER_WINDOW_BYTES", 32 * 1024 * 1024)
            .max(1024 * 1024)
            .min(max_bytes);
        Self::new(
            max_bytes,
            window_bytes,
            Duration::from_secs(env_limit("BLOCK_TTL_SECS", 600)),
        )
    }

    fn new(max_bytes: u64, window_bytes: u64, ttl: Duration) -> Self {
        let state = Arc::new(RwLock::new(State::default()));
        let weak = Arc::downgrade(&state);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let Some(state) = weak.upgrade() else { break };
                state.write().await.cleanup(Instant::now());
            }
        });
        Self {
            state,
            max_bytes,
            window_bytes,
            ttl,
        }
    }

    /// false means backpressure; callers should retry without allocating more chunks.
    pub async fn insert(&self, id: &str, block: FileBlock) -> bool {
        let mut state = self.state.write().await;
        let now = Instant::now();
        state.cleanup(now);
        let entry = state
            .transfers
            .entry(id.to_owned())
            .or_insert_with(|| Entry {
                transfer: Transfer::default(),
                expires: now + self.ttl,
            });
        entry.expires = now + self.ttl;
        let transfer = &entry.transfer;
        if transfer.done
            || block.start < transfer.next_start
            || transfer.acknowledged.contains_key(&block.start)
        {
            return true; // A late upload retry must not recreate acknowledged blocks.
        }
        if block.end >= transfer.next_start.saturating_add(self.window_bytes) {
            return false;
        }
        let key = (id.to_owned(), block.start);
        let old_bytes = state
            .blocks
            .get(&key)
            .map_or(0, |old| old.data.len() as u64);
        let new_bytes = block.data.len() as u64;
        let transfer_bytes = state.transfers[id].transfer.bytes - old_bytes + new_bytes;
        let total_bytes = state.bytes - old_bytes + new_bytes;
        if transfer_bytes > self.window_bytes || total_bytes > self.max_bytes {
            return false;
        }
        state.blocks.insert(key, block);
        state.transfers.get_mut(id).unwrap().transfer.bytes = transfer_bytes;
        state.bytes = total_bytes;
        true
    }

    pub async fn get(&self, id: &str, start: u64) -> Option<FileBlock> {
        let mut state = self.state.write().await;
        let now = Instant::now();
        state.cleanup(now);
        if let Some(entry) = state.transfers.get_mut(id) {
            entry.expires = now + self.ttl;
        }
        state.blocks.get(&(id.to_owned(), start)).cloned()
    }

    pub async fn acknowledge(&self, id: &str, start: u64) -> Result<(), &'static str> {
        let mut state = self.state.write().await;
        let now = Instant::now();
        state.cleanup(now);
        let entry = state.transfers.get_mut(id).ok_or("Block not found")?;
        entry.expires = now + self.ttl;
        if entry.transfer.done
            || start < entry.transfer.next_start
            || entry.transfer.acknowledged.contains_key(&start)
        {
            return Ok(());
        }
        let block = state
            .blocks
            .remove(&(id.to_owned(), start))
            .ok_or("Block not found")?;
        let bytes = block.data.len() as u64;
        state.bytes -= bytes;
        let transfer = &mut state.transfers.get_mut(id).unwrap().transfer;
        transfer.bytes -= bytes;
        transfer.acknowledged.insert(start, block.end + 1);
        while let Some(next) = transfer.acknowledged.remove(&transfer.next_start) {
            transfer.next_start = next;
        }
        Ok(())
    }

    pub async fn finish(&self, id: &str) {
        let mut state = self.state.write().await;
        let entry = state
            .transfers
            .entry(id.to_owned())
            .or_insert_with(|| Entry {
                transfer: Transfer::default(),
                expires: Instant::now() + self.ttl,
            });
        let released = entry.transfer.bytes;
        entry.transfer.bytes = 0;
        entry.transfer.done = true;
        entry.transfer.acknowledged.clear();
        state.bytes -= released;
        state.blocks.retain(|(key, _), _| key != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Bytes;

    fn block(start: u64) -> FileBlock {
        FileBlock::new(
            &Bytes::from_static(b"data"),
            "test".into(),
            start,
            start + 3,
            100,
        )
    }

    #[tokio::test]
    async fn window_moves_only_after_contiguous_acknowledgements() {
        let cache = BlockCache::new(16, 8, Duration::from_secs(60));
        assert!(cache.insert("a", block(0)).await);
        assert!(cache.insert("a", block(4)).await);
        assert!(!cache.insert("a", block(8)).await);
        cache.acknowledge("a", 4).await.unwrap();
        assert!(!cache.insert("a", block(8)).await);
        cache.acknowledge("a", 0).await.unwrap();
        assert!(cache.insert("a", block(8)).await);
        cache.acknowledge("a", 0).await.unwrap();
        assert!(cache.insert("a", block(0)).await);
        assert!(cache.get("a", 0).await.is_none());
        assert_eq!(cache.state.read().await.bytes, 4);
    }

    #[tokio::test]
    async fn concurrent_uploads_cannot_exceed_global_budget() {
        let cache = Arc::new(BlockCache::new(8, 8, Duration::from_secs(60)));
        let mut tasks = Vec::new();
        for index in 0..20 {
            let cache = cache.clone();
            tasks.push(tokio::spawn(async move {
                cache.insert(&index.to_string(), block(0)).await
            }));
        }
        let mut accepted = 0;
        for task in tasks {
            if task.await.unwrap() {
                accepted += 1;
            }
        }
        assert_eq!(accepted, 2);
        assert_eq!(cache.state.read().await.bytes, 8);
    }

    #[tokio::test]
    async fn retries_replace_without_double_counting_and_completion_releases_cache() {
        let cache = BlockCache::new(8, 8, Duration::from_secs(60));
        assert!(cache.insert("a", block(0)).await);
        assert!(cache.insert("a", block(0)).await);
        assert_eq!(cache.state.read().await.bytes, 4);
        cache.finish("a").await;
        assert!(cache.insert("a", block(0)).await);
        assert!(cache.get("a", 0).await.is_none());
        assert_eq!(cache.state.read().await.bytes, 0);
        assert!(cache.insert("b", block(0)).await);
    }

    #[tokio::test]
    async fn expiration_releases_the_budget() {
        let cache = BlockCache::new(4, 4, Duration::from_secs(60));
        assert!(cache.insert("a", block(0)).await);
        cache
            .state
            .write()
            .await
            .cleanup(Instant::now() + Duration::from_secs(61));
        assert_eq!(cache.state.read().await.bytes, 0);
        assert!(cache.insert("b", block(0)).await);
    }
}
