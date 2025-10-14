//! In-memory cache for hot blobs

use super::{BlobId, BlobStore};
use anyhow::Result;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// LRU cache for frequently accessed blobs
pub struct BlobCache {
    store: Arc<BlobStore>,
    cache: Arc<RwLock<LruCache<BlobId, Vec<u8>>>>,
    max_blob_size: usize,
}

impl BlobCache {
    pub fn new(store: Arc<BlobStore>, capacity: usize, max_blob_size: usize) -> Self {
        Self {
            store,
            cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap(),
            ))),
            max_blob_size,
        }
    }

    pub async fn get(&self, id: &BlobId) -> Result<Vec<u8>> {
        // Check cache first
        {
            let mut cache = self.cache.write().await;
            if let Some(data) = cache.get(id) {
                return Ok(data.clone());
            }
        }

        // Fetch from store
        let data = self.store.get(id).await?;

        // Cache if small enough
        if data.len() <= self.max_blob_size {
            let mut cache = self.cache.write().await;
            cache.put(id.clone(), data.clone());
        }

        Ok(data)
    }

    pub async fn put(&self, data: &[u8], compression: super::Compression) -> Result<BlobId> {
        let id = self.store.put(data, compression).await?;

        // Cache if small enough
        if data.len() <= self.max_blob_size {
            let mut cache = self.cache.write().await;
            cache.put(id.clone(), data.to_vec());
        }

        Ok(id)
    }

    pub async fn invalidate(&self, id: &BlobId) {
        let mut cache = self.cache.write().await;
        cache.pop(id);
    }
}
