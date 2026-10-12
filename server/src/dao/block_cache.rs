use super::db::FileBlock;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    env,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_PENDING_BLOCKS: usize = 4;
const DEMAND_TTL: Duration = Duration::from_secs(30);
const RATE_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);
const STALE_RECEIVER: Duration = Duration::from_secs(5);

struct Demand {
    length: u64,
    seen: Instant,
}
struct Lease {
    token: u64,
    length: u64,
    expires: Instant,
    claimed: bool,
}

struct Transfer {
    bytes: u64,
    next_start: u64,
    acknowledged: HashMap<u64, u64>,
    done: bool,
    window: u64,
    rate: f64,
    sample_bytes: u64,
    sample_start: Instant,
    last_progress: Instant,
    pending: BTreeMap<u64, Demand>,
    leases: BTreeMap<u64, Lease>,
}

impl Transfer {
    fn new(now: Instant, floor: u64) -> Self {
        Self {
            bytes: 0,
            next_start: 0,
            acknowledged: HashMap::new(),
            done: false,
            window: floor,
            rate: 0.0,
            sample_bytes: 0,
            sample_start: now,
            last_progress: now,
            pending: BTreeMap::new(),
            leases: BTreeMap::new(),
        }
    }
    fn used(&self) -> u64 {
        self.bytes + self.leases.values().map(|lease| lease.length).sum::<u64>()
    }
    fn acknowledged(&self, start: u64) -> bool {
        self.done || start < self.next_start || self.acknowledged.contains_key(&start)
    }
    fn active(&self, now: Instant) -> bool {
        !self.done
            && (self.used() > 0
                || !self.pending.is_empty()
                || now.duration_since(self.last_progress) < DEMAND_TTL)
    }
    fn eligible(&self, start: u64, length: u64) -> bool {
        start.saturating_add(length) <= self.next_start.saturating_add(self.window)
            && self.used().saturating_add(length) <= self.window
    }
    fn progress(&mut self, bytes: u64, now: Instant, limits: &Limits) {
        self.last_progress = now;
        self.sample_bytes += bytes;
        let elapsed = now.duration_since(self.sample_start);
        if elapsed >= RATE_SAMPLE_INTERVAL {
            let rate = self.sample_bytes as f64 / elapsed.as_secs_f64();
            self.rate = if self.rate == 0.0 {
                rate
            } else {
                self.rate * 0.75 + rate * 0.25
            };
            let target = (self.rate * 2.0) as u64;
            self.window = target
                .max(limits.floor)
                .min(limits.window)
                .min(self.window.saturating_mul(2));
            self.sample_bytes = 0;
            self.sample_start = now;
        }
    }
}

struct Entry {
    transfer: Transfer,
    expires: Instant,
}

#[derive(Default)]
struct State {
    blocks: HashMap<(String, u64), FileBlock>,
    transfers: HashMap<String, Entry>,
    queue: VecDeque<String>,
    bytes: u64, // Includes both cached blocks and all upload reservations.
    next_token: u64,
}

struct Limits {
    max_bytes: u64,
    window: u64,
    floor: u64,
    slots: usize,
    max_transfers: usize,
    ttl: Duration,
    permit_ttl: Duration,
}

impl State {
    fn cleanup(&mut self, now: Instant, limits: &Limits) {
        self.transfers.retain(|_, entry| {
            entry.expires > now || entry.transfer.leases.values().any(|lease| lease.claimed)
        });
        for entry in self.transfers.values_mut() {
            entry
                .transfer
                .leases
                .retain(|_, lease| lease.claimed || lease.expires > now);
            entry
                .transfer
                .pending
                .retain(|_, demand| now.duration_since(demand.seen) < DEMAND_TTL);
            if now.duration_since(entry.transfer.last_progress) >= STALE_RECEIVER {
                entry.transfer.window = limits.floor;
            }
        }
        self.blocks
            .retain(|(id, _), _| self.transfers.contains_key(id));
        self.queue.retain(|id| {
            self.transfers
                .get(id)
                .is_some_and(|entry| !entry.transfer.pending.is_empty())
        });
        self.bytes = self
            .transfers
            .values()
            .map(|entry| entry.transfer.used())
            .sum();
    }

    fn slots(&self) -> usize {
        self.transfers
            .values()
            .map(|entry| entry.transfer.leases.len())
            .sum()
    }

    fn schedule(&mut self, now: Instant, limits: &Limits) {
        let mut blocked = 0;
        while self.slots() < limits.slots && !self.queue.is_empty() && blocked < self.queue.len() {
            let id = self.queue.pop_front().unwrap();
            let transfer = &self.transfers[&id].transfer;
            let request = transfer
                .pending
                .iter()
                .find(|(start, demand)| transfer.eligible(**start, demand.length))
                .map(|(start, demand)| (*start, demand.length));
            let mut granted = false;
            if let Some((start, length)) = request {
                // Protect only actual waiting demand, never divide unused RAM into fixed shares.
                let protected: u64 = if transfer.used().saturating_add(length) > limits.floor {
                    self.transfers
                        .iter()
                        .filter(|(other, _)| *other != &id)
                        .map(|(_, entry)| {
                            let other = &entry.transfer;
                            let demand: u64 = other
                                .pending
                                .iter()
                                .filter(|(start, demand)| other.eligible(**start, demand.length))
                                .map(|(_, demand)| demand.length)
                                .sum();
                            limits.floor.saturating_sub(other.used()).min(demand)
                        })
                        .sum()
                } else {
                    0
                };
                if length
                    <= limits
                        .max_bytes
                        .saturating_sub(self.bytes)
                        .saturating_sub(protected)
                {
                    self.next_token = self
                        .next_token
                        .checked_add(1)
                        .expect("Upload token counter exhausted");
                    let transfer = &mut self.transfers.get_mut(&id).unwrap().transfer;
                    transfer.pending.remove(&start);
                    transfer.leases.insert(
                        start,
                        Lease {
                            token: self.next_token,
                            length,
                            expires: now + limits.permit_ttl,
                            claimed: false,
                        },
                    );
                    self.bytes += length;
                    granted = true;
                }
            }
            if !self.transfers[&id].transfer.pending.is_empty() {
                self.queue.push_back(id);
            }
            if granted {
                blocked = 0;
            } else {
                blocked += 1;
            }
        }
    }

    fn release(&mut self, id: &str, start: u64, token: u64) {
        if let Some(entry) = self.transfers.get_mut(id) {
            if entry
                .transfer
                .leases
                .get(&start)
                .is_some_and(|lease| lease.token == token)
            {
                let lease = entry.transfer.leases.remove(&start).unwrap();
                self.bytes -= lease.length;
            }
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Admission {
    Ready { token: String, expires_ms: u64 },
    Complete,
    Waiting { retry_after_ms: u64 },
}

/// The reservation remains charged while a request is reading its body.
/// Dropping a cancelled/failed request immediately releases its slot and RAM budget.
pub struct UploadGuard {
    state: Arc<Mutex<State>>,
    limits: Arc<Limits>,
    id: String,
    pub start: u64,
    pub length: u64,
    token: u64,
}

impl UploadGuard {
    pub fn commit(self, block: FileBlock) -> Result<(), &'static str> {
        if block.start != self.start
            || block.data.len() as u64 != self.length
            || block
                .end
                .checked_sub(block.start)
                .and_then(|length| length.checked_add(1))
                != Some(self.length)
        {
            return Err("Block does not match upload permit");
        }
        let mut state = self.state.lock().unwrap();
        let entry = state
            .transfers
            .get(&self.id)
            .ok_or("Upload permit expired")?;
        if !entry
            .transfer
            .leases
            .get(&self.start)
            .is_some_and(|lease| lease.token == self.token && lease.claimed)
        {
            return Err("Upload permit expired");
        }
        let complete = entry.transfer.acknowledged(self.start);
        state.release(&self.id, self.start, self.token);
        if !complete {
            state.bytes += self.length;
            let entry = state.transfers.get_mut(&self.id).unwrap();
            entry.transfer.bytes += self.length;
            entry.expires = Instant::now() + self.limits.ttl;
            state.blocks.insert((self.id.clone(), self.start), block);
        }
        state.schedule(Instant::now(), &self.limits);
        Ok(())
    }
}

impl Drop for UploadGuard {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.release(&self.id, self.start, self.token);
        let now = Instant::now();
        state.cleanup(now, &self.limits);
        state.schedule(now, &self.limits);
    }
}

/// RAM-only cache with adaptive windows and round-robin upload admission.
pub struct BlockCache {
    state: Arc<Mutex<State>>,
    limits: Arc<Limits>,
    pub max_bytes: u64,
    pub window_bytes: u64,
    pub floor_bytes: u64,
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
        let window = env_limit("TRANSFER_WINDOW_BYTES", 32 * 1024 * 1024)
            .max(1024 * 1024)
            .min(max_bytes);
        let floor = env_limit("TRANSFER_MIN_WINDOW_BYTES", 2 * 1024 * 1024)
            .max(1024 * 1024)
            .min(window);
        Self::with_limits(Limits {
            max_bytes,
            window,
            floor,
            slots: 4,
            max_transfers: env_limit("MAX_ACTIVE_HTTP_TRANSFERS", 128).min(1024) as usize,
            ttl: Duration::from_secs(env_limit("BLOCK_TTL_SECS", 600)),
            permit_ttl: Duration::from_secs(30),
        })
    }

    fn with_limits(limits: Limits) -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let limits = Arc::new(limits);
        let weak = Arc::downgrade(&state);
        let cleanup_limits = limits.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let Some(state) = weak.upgrade() else { break };
                let mut state = state.lock().unwrap();
                let now = Instant::now();
                state.cleanup(now, &cleanup_limits);
                state.schedule(now, &cleanup_limits);
            }
        });
        Self {
            state,
            max_bytes: limits.max_bytes,
            window_bytes: limits.window,
            floor_bytes: limits.floor,
            limits,
        }
    }

    pub async fn admit(&self, id: &str, start: u64, length: u64) -> Admission {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        state.cleanup(now, &self.limits);
        if length == 0 || length > self.limits.floor {
            return Admission::Waiting {
                retry_after_ms: 500,
            };
        }
        if state
            .transfers
            .get(id)
            .is_some_and(|entry| entry.transfer.acknowledged(start))
            || state.blocks.contains_key(&(id.to_owned(), start))
        {
            return Admission::Complete;
        }
        let active = state
            .transfers
            .values()
            .filter(|entry| entry.transfer.active(now))
            .count();
        if !state
            .transfers
            .get(id)
            .is_some_and(|entry| entry.transfer.active(now))
            && active >= self.limits.max_transfers
        {
            return Admission::Waiting {
                retry_after_ms: 1000,
            };
        }
        let entry = state
            .transfers
            .entry(id.to_owned())
            .or_insert_with(|| Entry {
                transfer: Transfer::new(now, self.limits.floor),
                expires: now + self.limits.ttl,
            });
        entry.expires = now + self.limits.ttl;
        if entry.transfer.acknowledged(start) || state.blocks.contains_key(&(id.to_owned(), start))
        {
            return Admission::Complete;
        }
        let transfer = &mut state.transfers.get_mut(id).unwrap().transfer;
        if let Some(lease) = transfer.leases.get(&start) {
            if lease.length == length && !lease.claimed {
                return Admission::Ready {
                    token: lease.token.to_string(),
                    expires_ms: lease.expires.saturating_duration_since(now).as_millis() as u64,
                };
            }
            return Admission::Waiting {
                retry_after_ms: 500,
            };
        }
        if transfer.pending.contains_key(&start) || transfer.pending.len() < MAX_PENDING_BLOCKS {
            transfer.pending.insert(start, Demand { length, seen: now });
            if !state.queue.iter().any(|queued| queued == id) {
                state.queue.push_back(id.to_owned());
            }
        }
        state.schedule(now, &self.limits);
        if let Some(lease) = state.transfers[id].transfer.leases.get(&start) {
            Admission::Ready {
                token: lease.token.to_string(),
                expires_ms: self.limits.permit_ttl.as_millis() as u64,
            }
        } else {
            Admission::Waiting {
                retry_after_ms: 500,
            }
        }
    }

    pub fn begin_upload(&self, id: &str, token: &str) -> Result<UploadGuard, &'static str> {
        let token = token.parse::<u64>().map_err(|_| "Invalid upload permit")?;
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        state.cleanup(now, &self.limits);
        let entry = state.transfers.get_mut(id).ok_or("Upload permit expired")?;
        let (&start, lease) = entry
            .transfer
            .leases
            .iter_mut()
            .find(|(_, lease)| lease.token == token)
            .ok_or("Upload permit expired")?;
        if lease.claimed {
            return Err("Upload permit already in use");
        }
        lease.claimed = true;
        let length = lease.length;
        entry.expires = now + self.limits.ttl;
        Ok(UploadGuard {
            state: self.state.clone(),
            limits: self.limits.clone(),
            id: id.to_owned(),
            start,
            length,
            token,
        })
    }

    #[cfg(test)]
    fn new(max_bytes: u64, window: u64, ttl: Duration) -> Self {
        Self::with_limits(Limits {
            max_bytes,
            window,
            floor: window.min(2 * 1024 * 1024),
            slots: 4,
            max_transfers: 128,
            ttl,
            permit_ttl: Duration::from_secs(30),
        })
    }

    #[cfg(test)]
    pub async fn insert(&self, id: &str, block: FileBlock) -> bool {
        match self.admit(id, block.start, block.data.len() as u64).await {
            Admission::Complete => true,
            Admission::Ready { token, .. } => {
                self.begin_upload(id, &token).unwrap().commit(block).is_ok()
            }
            Admission::Waiting { .. } => false,
        }
    }

    pub async fn get(&self, id: &str, start: u64) -> Option<FileBlock> {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        state.cleanup(now, &self.limits);
        if let Some(entry) = state.transfers.get_mut(id) {
            entry.expires = now + self.limits.ttl;
        }
        state.blocks.get(&(id.to_owned(), start)).cloned()
    }

    pub async fn acknowledge(&self, id: &str, start: u64) -> Result<(), &'static str> {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        state.cleanup(now, &self.limits);
        let entry = state.transfers.get_mut(id).ok_or("Block not found")?;
        entry.expires = now + self.limits.ttl;
        if entry.transfer.acknowledged(start) {
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
        transfer.progress(bytes, now, &self.limits);
        state.schedule(now, &self.limits);
        Ok(())
    }

    pub async fn finish(&self, id: &str) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let entry = state
            .transfers
            .entry(id.to_owned())
            .or_insert_with(|| Entry {
                transfer: Transfer::new(now, self.limits.floor),
                expires: now + self.limits.ttl,
            });
        let transfer = &mut entry.transfer;
        let released = transfer.bytes
            + transfer
                .leases
                .values()
                .filter(|lease| !lease.claimed)
                .map(|lease| lease.length)
                .sum::<u64>();
        transfer.bytes = 0;
        transfer.done = true;
        transfer.acknowledged.clear();
        transfer.pending.clear();
        transfer.leases.retain(|_, lease| lease.claimed);
        state.bytes -= released;
        state.blocks.retain(|(key, _), _| key != id);
        state.queue.retain(|queued| queued != id);
        state.schedule(now, &self.limits);
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
        assert!(cache.state.lock().unwrap().bytes <= 16);
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
        assert_eq!(cache.state.lock().unwrap().bytes, 8);
    }
    #[tokio::test]
    async fn retries_replace_without_double_counting_and_completion_releases_cache() {
        let cache = BlockCache::new(8, 8, Duration::from_secs(60));
        assert!(cache.insert("a", block(0)).await);
        assert!(cache.insert("a", block(0)).await);
        assert_eq!(cache.state.lock().unwrap().bytes, 4);
        cache.finish("a").await;
        assert!(cache.insert("a", block(0)).await);
        assert!(cache.get("a", 0).await.is_none());
        assert_eq!(cache.state.lock().unwrap().bytes, 0);
        assert!(cache.insert("b", block(0)).await);
    }
    #[tokio::test]
    async fn expiration_releases_the_budget() {
        let cache = BlockCache::new(4, 4, Duration::from_secs(60));
        assert!(cache.insert("a", block(0)).await);
        cache
            .state
            .lock()
            .unwrap()
            .cleanup(Instant::now() + Duration::from_secs(61), &cache.limits);
        assert_eq!(cache.state.lock().unwrap().bytes, 0);
        assert!(cache.insert("b", block(0)).await);
    }

    fn test_limits(slots: usize) -> Limits {
        Limits {
            max_bytes: 128,
            window: 64,
            floor: 4,
            slots,
            max_transfers: 128,
            ttl: Duration::from_secs(60),
            permit_ttl: Duration::from_secs(30),
        }
    }

    fn ready(admission: Admission) -> String {
        match admission {
            Admission::Ready { token, .. } => token,
            other => panic!("Expected permit, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn thirty_two_transfers_get_a_turn_before_a_borrower_refills() {
        let cache = BlockCache::with_limits(test_limits(1));
        let token = ready(cache.admit("borrower", 0, 4).await);
        let first = cache.begin_upload("borrower", &token).unwrap();
        cache
            .state
            .lock()
            .unwrap()
            .transfers
            .get_mut("borrower")
            .unwrap()
            .transfer
            .window = 64;
        assert!(matches!(
            cache.admit("borrower", 4, 4).await,
            Admission::Waiting { .. }
        ));
        for index in 0..31 {
            assert!(matches!(
                cache.admit(&format!("user-{index}"), 0, 4).await,
                Admission::Waiting { .. }
            ));
        }
        first.commit(block(0)).unwrap();
        for index in 0..31 {
            let id = format!("user-{index}");
            let token = ready(cache.admit(&id, 0, 4).await);
            cache
                .begin_upload(&id, &token)
                .unwrap()
                .commit(block(0))
                .unwrap();
            assert!(cache.state.lock().unwrap().bytes <= 128);
        }
        assert_eq!(cache.state.lock().unwrap().bytes, 128);
        cache.acknowledge("user-0", 0).await.unwrap();
        let token = ready(cache.admit("borrower", 4, 4).await);
        cache
            .begin_upload("borrower", &token)
            .unwrap()
            .commit(block(4))
            .unwrap();
    }

    #[tokio::test]
    async fn idle_transfers_do_not_force_equal_shares() {
        let cache = BlockCache::with_limits(test_limits(4));
        for index in 0..31 {
            let id = format!("idle-{index}");
            assert!(cache.insert(&id, block(0)).await);
            cache.acknowledge(&id, 0).await.unwrap();
        }
        let token = ready(cache.admit("fast", 0, 4).await);
        cache
            .state
            .lock()
            .unwrap()
            .transfers
            .get_mut("fast")
            .unwrap()
            .transfer
            .window = 32;
        cache
            .begin_upload("fast", &token)
            .unwrap()
            .commit(block(0))
            .unwrap();
        for start in (4..32).step_by(4) {
            assert!(cache.insert("fast", block(start)).await);
        }
        let state = cache.state.lock().unwrap();
        assert_eq!(state.transfers.len(), 32);
        assert_eq!(state.transfers["fast"].transfer.bytes, 32); // Eight times the equal share.
        assert_eq!(state.bytes, 32);
    }

    #[tokio::test]
    async fn permits_reserve_bytes_and_cancellation_releases_them() {
        let cache = BlockCache::with_limits(test_limits(1));
        let token = ready(cache.admit("a", 0, 4).await);
        assert_eq!(cache.state.lock().unwrap().bytes, 4);
        assert!(cache.begin_upload("other", &token).is_err());
        let upload = cache.begin_upload("a", &token).unwrap();
        assert!(cache.begin_upload("a", &token).is_err());
        assert!(matches!(
            cache.admit("b", 0, 4).await,
            Admission::Waiting { .. }
        ));
        drop(upload);
        let token = ready(cache.admit("b", 0, 4).await);
        cache
            .begin_upload("b", &token)
            .unwrap()
            .commit(block(0))
            .unwrap();
        assert_eq!(cache.state.lock().unwrap().bytes, 4);
    }

    #[tokio::test]
    async fn unused_permits_expire_but_active_body_readers_remain_charged() {
        let cache = BlockCache::with_limits(test_limits(1));
        let token = ready(cache.admit("a", 0, 4).await);
        cache
            .state
            .lock()
            .unwrap()
            .cleanup(Instant::now() + Duration::from_secs(31), &cache.limits);
        assert_eq!(cache.state.lock().unwrap().bytes, 0);
        assert!(cache.begin_upload("a", &token).is_err());
        let token = ready(cache.admit("b", 0, 4).await);
        let upload = cache.begin_upload("b", &token).unwrap();
        cache
            .state
            .lock()
            .unwrap()
            .cleanup(Instant::now() + Duration::from_secs(61), &cache.limits);
        assert_eq!(cache.state.lock().unwrap().bytes, 4);
        drop(upload);
        assert_eq!(cache.state.lock().unwrap().bytes, 0);
    }

    #[tokio::test]
    async fn receiver_throughput_grows_and_shrinks_the_window() {
        let limits = test_limits(4);
        let mut now = Instant::now();
        let mut transfer = Transfer::new(now, limits.floor);
        for _ in 0..4 {
            now += Duration::from_millis(250);
            transfer.progress(128, now, &limits);
        }
        assert_eq!(transfer.window, 64);
        for _ in 0..32 {
            now += Duration::from_secs(1);
            transfer.progress(1, now, &limits);
        }
        assert_eq!(transfer.window, limits.floor);
    }
}
