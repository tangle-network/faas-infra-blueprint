use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;

/// Multi-layer cache manager for maximum performance
/// Implements L1 (memory) -> L2 (disk) -> L3 (distributed) hierarchy
pub struct CacheManager {
    l1_cache: Arc<RwLock<MemoryCache>>,
    l2_cache: Arc<RwLock<DiskCache>>,
    strategy: CacheStrategy,
    metrics: Arc<RwLock<CacheMetrics>>,
}

#[derive(Debug, Clone)]
pub struct CacheStrategy {
    pub l1_max_size: usize,
    pub l1_ttl: Duration,
    pub l2_max_size: usize,
    pub l2_ttl: Duration,
    pub eviction_policy: EvictionPolicy,
    pub compression: bool,
}

#[derive(Debug, Clone)]
pub enum EvictionPolicy {
    LRU,
    LFU,
    FIFO,
    Adaptive, // Adapts between LRU and LFU based on access patterns
}

#[derive(Debug)]
struct MemoryCache {
    entries: HashMap<String, CacheEntry>,
    access_order: Vec<String>,
    frequency: HashMap<String, u32>,
    max_size: usize,
    current_size: usize,
}

#[derive(Debug)]
struct DiskCache {
    base_path: std::path::PathBuf,
    index: HashMap<String, DiskEntry>,
    max_size: usize,
    current_size: usize,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    data: Arc<Vec<u8>>,
    created_at: Instant,
    last_accessed: Instant,
    access_count: u32,
    size: usize,
    metadata: CacheMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    content_type: String,
    compression: Option<String>,
    dependencies: Vec<String>,
    checksum: String,
}

#[derive(Debug, Clone)]
struct DiskEntry {
    file_path: std::path::PathBuf,
    created_at: SystemTime,
    last_accessed: SystemTime,
    size: usize,
    metadata: CacheMetadata,
}

#[derive(Debug, Default, Clone)]
pub struct CacheMetrics {
    pub l1_hits: u64,
    pub l1_misses: u64,
    pub l2_hits: u64,
    pub l2_misses: u64,
    pub evictions: u64,
    pub total_size: usize,
    pub avg_access_time: Duration,
}

impl Default for CacheStrategy {
    fn default() -> Self {
        Self {
            l1_max_size: 100 * 1024 * 1024,     // 100MB
            l1_ttl: Duration::from_secs(3600),  // 1 hour
            l2_max_size: 1024 * 1024 * 1024,    // 1GB
            l2_ttl: Duration::from_secs(86400), // 24 hours
            eviction_policy: EvictionPolicy::Adaptive,
            compression: true,
        }
    }
}

impl CacheManager {
    pub async fn new(strategy: CacheStrategy) -> Result<Self> {
        let cache_dir = std::path::PathBuf::from("/tmp/faas-cache");
        tokio::fs::create_dir_all(&cache_dir).await?;

        Ok(Self {
            l1_cache: Arc::new(RwLock::new(MemoryCache::new(strategy.l1_max_size))),
            l2_cache: Arc::new(RwLock::new(DiskCache::new(
                cache_dir,
                strategy.l2_max_size,
            )?)),
            strategy,
            metrics: Arc::new(RwLock::new(CacheMetrics::default())),
        })
    }

    /// Get data from cache with automatic promotion from L2 to L1
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let start = Instant::now();

        // Try L1 cache first
        if let Some(data) = self.get_from_l1(key).await? {
            self.record_l1_hit(start.elapsed()).await;
            return Ok(Some(data));
        }

        // Try L2 cache
        if let Some(data) = self.get_from_l2(key).await? {
            // Promote to L1 for faster future access
            self.put_to_l1(key, &data).await?;
            self.record_l2_hit(start.elapsed()).await;
            return Ok(Some(data));
        }

        self.record_miss(start.elapsed()).await;
        Ok(None)
    }

    /// Put data into cache with intelligent placement
    pub async fn put(
        &self,
        key: &str,
        data: Vec<u8>,
        metadata: Option<CacheMetadata>,
    ) -> Result<()> {
        let metadata = metadata.unwrap_or_else(|| self.generate_metadata(&data));

        // Always put in L1 for immediate access
        self.put_to_l1_with_metadata(key, &data, metadata.clone())
            .await?;

        // Also store in L2 for persistence (async)
        let l2_cache = self.l2_cache.clone();
        let key = key.to_string();
        let data_clone = data.clone();
        tokio::spawn(async move {
            let _ = l2_cache.write().await.put(&key, data_clone, metadata).await;
        });

        Ok(())
    }

    /// Intelligent cache warming based on dependency analysis
    pub async fn warm_dependencies(&self, keys: Vec<String>) -> Result<()> {
        let mut tasks = Vec::new();

        for key in keys {
            let cache = self.clone();
            tasks.push(tokio::spawn(async move {
                // Pre-load from L2 to L1 if available
                if let Ok(Some(data)) = cache.get_from_l2(&key).await {
                    let _ = cache.put_to_l1(&key, &data).await;
                }
            }));
        }

        futures::future::join_all(tasks).await;
        Ok(())
    }

    /// Batch get operation for better performance
    pub async fn get_batch(&self, keys: Vec<String>) -> Result<HashMap<String, Vec<u8>>> {
        let mut results = HashMap::new();
        let mut tasks = Vec::new();

        // Process in parallel
        for key in keys {
            let cache = self.clone();
            tasks.push(tokio::spawn(
                async move { (key.clone(), cache.get(&key).await) },
            ));
        }

        let task_results = futures::future::join_all(tasks).await;

        for task_result in task_results {
            if let Ok((key, Ok(Some(data)))) = task_result {
                results.insert(key, data);
            }
        }

        Ok(results)
    }

    async fn get_from_l1(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let mut cache = self.l1_cache.write().await;

        if let Some(entry) = cache.entries.get_mut(key) {
            // Update access statistics
            entry.last_accessed = Instant::now();
            entry.access_count += 1;

            // Clone the data first
            let data = entry.data.as_ref().clone();

            // Update frequency tracking
            *cache.frequency.entry(key.to_string()).or_insert(0) += 1;

            // Update LRU order
            if let Some(pos) = cache.access_order.iter().position(|k| k == key) {
                cache.access_order.remove(pos);
            }
            cache.access_order.push(key.to_string());

            return Ok(Some(data));
        }

        Ok(None)
    }

    async fn get_from_l2(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let mut cache = self.l2_cache.write().await;

        if let Some(entry) = cache.index.get_mut(key) {
            // Check if file still exists and is valid
            if entry.file_path.exists() {
                let data = tokio::fs::read(&entry.file_path).await?;

                // Verify checksum
                if self.verify_checksum(&data, &entry.metadata.checksum) {
                    entry.last_accessed = SystemTime::now();
                    return Ok(Some(data));
                } else {
                    // Corrupted file, remove it
                    let _ = tokio::fs::remove_file(&entry.file_path).await;
                    cache.index.remove(key);
                }
            } else {
                // File missing, clean up index
                cache.index.remove(key);
            }
        }

        Ok(None)
    }

    async fn put_to_l1(&self, key: &str, data: &[u8]) -> Result<()> {
        let metadata = self.generate_metadata(data);
        self.put_to_l1_with_metadata(key, data, metadata).await
    }

    async fn put_to_l1_with_metadata(
        &self,
        key: &str,
        data: &[u8],
        metadata: CacheMetadata,
    ) -> Result<()> {
        let mut cache = self.l1_cache.write().await;

        // Check if we need to evict first
        let entry_size = data.len();
        while cache.current_size + entry_size > cache.max_size && !cache.entries.is_empty() {
            self.evict_from_l1(&mut cache).await?;
        }

        let entry = CacheEntry {
            data: Arc::new(data.to_vec()),
            created_at: Instant::now(),
            last_accessed: Instant::now(),
            access_count: 0,
            size: entry_size,
            metadata,
        };

        cache.entries.insert(key.to_string(), entry);
        cache.current_size += entry_size;
        cache.access_order.push(key.to_string());
        cache.frequency.insert(key.to_string(), 1);

        Ok(())
    }

    async fn evict_from_l1(&self, cache: &mut MemoryCache) -> Result<()> {
        let key_to_evict = match self.strategy.eviction_policy {
            EvictionPolicy::LRU => cache.access_order.first().cloned(),
            EvictionPolicy::LFU => cache
                .frequency
                .iter()
                .min_by_key(|(_, &freq)| freq)
                .map(|(key, _)| key.clone()),
            EvictionPolicy::FIFO => cache.entries.keys().next().cloned(),
            EvictionPolicy::Adaptive => {
                // Choose between LRU and LFU based on hit pattern
                if cache.entries.len() > 100 {
                    // Use LFU for larger caches
                    cache
                        .frequency
                        .iter()
                        .min_by_key(|(_, &freq)| freq)
                        .map(|(key, _)| key.clone())
                } else {
                    // Use LRU for smaller caches
                    cache.access_order.first().cloned()
                }
            }
        };

        if let Some(key) = key_to_evict {
            if let Some(entry) = cache.entries.remove(&key) {
                cache.current_size -= entry.size;
                cache.frequency.remove(&key);
                if let Some(pos) = cache.access_order.iter().position(|k| k == &key) {
                    cache.access_order.remove(pos);
                }

                // Update metrics
                let mut metrics = self.metrics.write().await;
                metrics.evictions += 1;
            }
        }

        Ok(())
    }

    fn generate_metadata(&self, data: &[u8]) -> CacheMetadata {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let checksum = format!("{:x}", hasher.finalize());

        CacheMetadata {
            content_type: "application/octet-stream".to_string(),
            compression: if self.strategy.compression {
                Some("gzip".to_string())
            } else {
                None
            },
            dependencies: Vec::new(),
            checksum,
        }
    }

    fn verify_checksum(&self, data: &[u8], expected: &str) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let actual = format!("{:x}", hasher.finalize());
        actual == expected
    }

    async fn record_l1_hit(&self, access_time: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.l1_hits += 1;
        metrics.avg_access_time = (metrics.avg_access_time + access_time) / 2;
    }

    async fn record_l2_hit(&self, access_time: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.l2_hits += 1;
        metrics.avg_access_time = (metrics.avg_access_time + access_time) / 2;
    }

    async fn record_miss(&self, access_time: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.l1_misses += 1;
        metrics.l2_misses += 1;
        metrics.avg_access_time = (metrics.avg_access_time + access_time) / 2;
    }

    pub async fn get_metrics(&self) -> CacheMetrics {
        self.metrics.read().await.clone()
    }

    /// Cleanup expired entries
    pub async fn cleanup(&self) -> Result<()> {
        // Cleanup L1
        {
            let mut l1 = self.l1_cache.write().await;
            let now = Instant::now();
            let expired_keys: Vec<String> = l1
                .entries
                .iter()
                .filter(|(_, entry)| now.duration_since(entry.created_at) > self.strategy.l1_ttl)
                .map(|(key, _)| key.clone())
                .collect();

            for key in expired_keys {
                if let Some(entry) = l1.entries.remove(&key) {
                    l1.current_size -= entry.size;
                    l1.frequency.remove(&key);
                    if let Some(pos) = l1.access_order.iter().position(|k| k == &key) {
                        l1.access_order.remove(pos);
                    }
                }
            }
        }

        // Cleanup L2
        {
            let mut l2 = self.l2_cache.write().await;
            let now = SystemTime::now();
            let expired_keys: Vec<String> = l2
                .index
                .iter()
                .filter(|(_, entry)| {
                    now.duration_since(entry.created_at)
                        .unwrap_or(Duration::ZERO)
                        > self.strategy.l2_ttl
                })
                .map(|(key, _)| key.clone())
                .collect();

            for key in expired_keys {
                if let Some(entry) = l2.index.remove(&key) {
                    let _ = tokio::fs::remove_file(&entry.file_path).await;
                    l2.current_size -= entry.size;
                }
            }
        }

        Ok(())
    }
}

impl Clone for CacheManager {
    fn clone(&self) -> Self {
        Self {
            l1_cache: self.l1_cache.clone(),
            l2_cache: self.l2_cache.clone(),
            strategy: self.strategy.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

impl MemoryCache {
    fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            access_order: Vec::new(),
            frequency: HashMap::new(),
            max_size,
            current_size: 0,
        }
    }
}

impl DiskCache {
    fn new(base_path: std::path::PathBuf, max_size: usize) -> Result<Self> {
        std::fs::create_dir_all(&base_path)?;

        Ok(Self {
            base_path,
            index: HashMap::new(),
            max_size,
            current_size: 0,
        })
    }

    async fn put(&mut self, key: &str, data: Vec<u8>, metadata: CacheMetadata) -> Result<()> {
        let file_name = format!("{}.cache", Self::hash_key(key));
        let file_path = self.base_path.join(file_name);

        tokio::fs::write(&file_path, &data).await?;

        let entry = DiskEntry {
            file_path,
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            size: data.len(),
            metadata,
        };

        self.index.insert(key.to_string(), entry);
        self.current_size += data.len();

        // Evict if over capacity
        while self.current_size > self.max_size && !self.index.is_empty() {
            self.evict_oldest().await?;
        }

        Ok(())
    }

    async fn evict_oldest(&mut self) -> Result<()> {
        if let Some((key, entry)) = self
            .index
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(k, v)| (k.clone(), v.clone()))
        {
            let _ = tokio::fs::remove_file(&entry.file_path).await;
            self.index.remove(&key);
            self.current_size -= entry.size;
        }
        Ok(())
    }

    fn hash_key(key: &str) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache = CacheManager::new(CacheStrategy::default()).await.unwrap();

        let test_data = b"Hello, World!".to_vec();
        cache
            .put("test_key", test_data.clone(), None)
            .await
            .unwrap();

        let retrieved = cache.get("test_key").await.unwrap();
        assert_eq!(retrieved, Some(test_data));
    }

    #[tokio::test]
    async fn test_cache_batch_operations() {
        let cache = CacheManager::new(CacheStrategy::default()).await.unwrap();

        // Put multiple entries
        for i in 0..5 {
            let data = format!("data_{}", i).into_bytes();
            cache.put(&format!("key_{}", i), data, None).await.unwrap();
        }

        // Batch get
        let keys = (0..5).map(|i| format!("key_{}", i)).collect();
        let results = cache.get_batch(keys).await.unwrap();

        assert_eq!(results.len(), 5);
        assert_eq!(results.get("key_0").unwrap(), b"data_0");
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let mut strategy = CacheStrategy::default();
        strategy.l1_max_size = 100; // Very small cache

        let cache = CacheManager::new(strategy).await.unwrap();

        // Fill cache beyond capacity
        for i in 0..10 {
            let data = vec![0u8; 50]; // 50 bytes each
            cache.put(&format!("key_{}", i), data, None).await.unwrap();
        }

        // Should have evicted earlier entries
        let metrics = cache.get_metrics().await;
        assert!(metrics.evictions > 0);
    }
}
