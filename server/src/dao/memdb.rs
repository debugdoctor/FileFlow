use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::RwLock;
use tokio::time::Duration;
use tracing::{event, instrument, Level};

pub struct MemDB<T> {
    pub store: Arc<RwLock<HashMap<String, CacheEntry<T>>>>,
}

#[derive(Clone)]
pub struct CacheEntry<T> {
    pub value: T,
    pub exp: Instant,
}

impl <T: Send + Sync + Clone + 'static> MemDB<T> {
    pub fn new() -> Self {
        let cache = MemDB {
            store: Arc::new(RwLock::new(HashMap::new())),
        };

        let store_clone = cache.store.clone();
        
        tokio::spawn(async move {
            let clean_up_interval = Duration::from_secs(1);
            let mut last_count: usize = 0;

            loop {
                tokio::time::sleep(clean_up_interval).await;
                let now = Instant::now();
                let mut store = store_clone.write().await;
                let count_before = store.len();
                store.retain(|_, entry| entry.exp > now);
                let count_after = store.len();
                
                // Only log when there are actual changes to reduce log noise
                // Changed from DEBUG to TRACE to reduce log verbosity
                if count_before != count_after || count_after != last_count {
                    event!(Level::INFO, "Cache cleanup: {} entries removed, {} entries remaining", 
                           count_before - count_after, count_after);
                    last_count = count_after;
                }
            }
        });

        cache
    }

    pub async fn insert(&self, key: &str, value: T, ttl_secs: u64) -> Result<(), String> {
        let exp = Instant::now() + Duration::from_secs(ttl_secs);
        let entry = CacheEntry { value, exp };
        
        self.store
            .write()
            .await
            .insert(key.to_owned(), entry);
            
        // Changed from DEBUG to TRACE to reduce log verbosity
        event!(Level::TRACE, "Inserted key: {} with TTL: {}s", key, ttl_secs);
        Ok(())
    }

    pub async fn update(&self, key: &str, value: T, exp: Instant) -> Result<(), String> {
        let entry = CacheEntry { value, exp };
        
        self.store
            .write()
            .await
            .insert(key.to_owned(), entry);
            
        // Changed from DEBUG to TRACE to reduce log verbosity
        event!(Level::TRACE, "Updated key: {}", key);
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Option<CacheEntry<T>> {
        let store = self.store.read().await;
        let result = store.get(key).cloned();
        
        // Changed from TRACE to VERBOSE TRACE (keeping as TRACE)
        if result.is_some() {
            event!(Level::TRACE, "Cache hit for key: {}", key);
        } else {
            event!(Level::TRACE, "Cache miss for key: {}", key);
        }
        
        result
    }

    pub async fn remove(&self, key: &str) -> Option<CacheEntry<T>> {
        let result = self.store
            .write()
            .await
            .remove(key);
            
        // Changed from DEBUG to TRACE to reduce log verbosity
        if result.is_some() {
            event!(Level::TRACE, "Removed key: {}", key);
        } else {
            event!(Level::TRACE, "Attempted to remove non-existent key: {}", key);
        }
        
        result
    }
}