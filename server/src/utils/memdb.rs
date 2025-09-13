use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::RwLock;
use tokio::time::Duration;

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

            loop {
                tokio::time::sleep(clean_up_interval).await;
                let now = Instant::now();
                store_clone.write().await.retain(|_, entry| entry.exp > now);
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
            
        Ok(())
    }

    pub async fn update(&self, key: &str, value: T, exp: Instant) -> Result<(), String> {

        let entry = CacheEntry { value, exp };
        
        self.store
            .write()
            .await
            .insert(key.to_owned(), entry);
            
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Option<CacheEntry<T>> {
        let store = self.store.read().await;
        store.get(key).cloned()
    }

    pub async fn remove(&self, key: &str) -> Option<CacheEntry<T>> {
        self.store
            .write()
            .await
            .remove(key)
    }
}