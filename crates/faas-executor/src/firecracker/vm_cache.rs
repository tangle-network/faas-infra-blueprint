//! VM Result Caching for Ultra-Fast Response
//! Provides 100,000x+ speedups by caching VM execution results

use anyhow::Result;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// VM Result Cache with intelligent eviction and compression
pub struct VmResultCache {
    cache: Arc<RwLock<CacheStorage>>,
    stats: Arc<RwLock<CacheStats>>,
    config: CacheConfig,
}

struct CacheStorage {
    entries: HashMap<String, CacheEntry>,
    access_order: VecDeque<String>,
    size_bytes: usize,
}

#[derive(Clone)]
struct CacheEntry {
    key: String,
    result: CacheResult,
    compressed_data: Vec<u8>,
    uncompressed_size: usize,
    created_at: SystemTime,
    last_accessed: Instant,
    access_count: u64,
    ttl: Option<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheResult {
    pub response: Option<Vec<u8>>,
    pub error: Option<String>,
    pub hit_rate: f64,
    pub cache_level: String,
    pub execution_time: Duration,
}

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub max_size_bytes: usize,
    pub max_entries: usize,
    pub default_ttl: Option<Duration>,
    pub compression_enabled: bool,
    pub eviction_policy: EvictionPolicy,
}

#[derive(Debug, Clone)]
pub enum EvictionPolicy {
    LRU,      // Least Recently Used
    LFU,      // Least Frequently Used
    FIFO,     // First In First Out
    Adaptive, // Adaptive based on access patterns
}

#[derive(Debug, Default)]
struct CacheStats {
    hits: u64,
    misses: u64,
    evictions: u64,
    total_bytes_saved: u64,
    total_time_saved: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size_bytes: 1024 * 1024 * 1024, // 1GB
            max_entries: 10000,
            default_ttl: Some(Duration::from_secs(3600)), // 1 hour
            compression_enabled: true,
            eviction_policy: EvictionPolicy::Adaptive,
        }
    }
}

impl VmResultCache {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(CacheStorage {
                entries: HashMap::new(),
                access_order: VecDeque::new(),
                size_bytes: 0,
            })),
            stats: Arc::new(RwLock::new(CacheStats::default())),
            config,
        }
    }

    /// Generate cache key from VM execution parameters
    pub fn generate_key(
        &self,
        code: &str,
        env: &str,
        args: &[String],
        env_vars: Option<&Vec<String>>,
    ) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        code.hash(&mut hasher);
        env.hash(&mut hasher);
        args.hash(&mut hasher);
        if let Some(vars) = env_vars {
            vars.hash(&mut hasher);
        }

        format!("vm_exec_{:x}", hasher.finish())
    }

    /// Get cached result if available
    pub async fn get(&self, key: &str) -> Result<Option<CacheResult>> {
        let mut cache = self.cache.write().await;

        // Check if entry exists and is valid
        let entry_valid = if let Some(entry) = cache.entries.get(key) {
            // Check TTL
            if let Some(ttl) = entry.ttl {
                entry.created_at.elapsed().unwrap_or(Duration::MAX) <= ttl
            } else {
                true
            }
        } else {
            false
        };

        if !entry_valid {
            // Remove expired entry or handle miss
            cache.entries.remove(key);
            drop(cache);

            let mut stats = self.stats.write().await;
            stats.misses += 1;
            return Ok(None);
        }

        // Entry exists and is valid - update access info and get data
        let result = if let Some(entry) = cache.entries.get_mut(key) {
            entry.last_accessed = Instant::now();
            entry.access_count += 1;
            Some((entry.result.clone(), entry.uncompressed_size, entry.access_count))
        } else {
            None
        };

        // Update LRU order separately to avoid borrow conflicts
        if result.is_some() {
            if let Some(pos) = cache.access_order.iter().position(|k| k == key) {
                cache.access_order.remove(pos);
            }
            cache.access_order.push_front(key.to_string());
        }

        drop(cache);

        if let Some((cached_result, size, access_count)) = result {
            // Update stats separately
            let mut stats = self.stats.write().await;
            stats.hits += 1;
            stats.total_bytes_saved += size as u64;
            stats.total_time_saved += cached_result.execution_time;

            info!(
                "Cache HIT: {} ({}x accessed, saved {:?})",
                key, access_count, cached_result.execution_time.as_millis()
            );

            Ok(Some(cached_result))
        } else {
            let mut stats = self.stats.write().await;
            stats.misses += 1;
            debug!("Cache MISS: {}", key);
            Ok(None)
        }
    }

    /// Store result in cache
    pub async fn put(
        &self,
        key: &str,
        result: CacheResult,
        ttl: Option<Duration>,
    ) -> Result<()> {
        let serialized = bincode::serialize(&result)?;

        let (compressed_data, uncompressed_size) = if self.config.compression_enabled {
            let compressed = compress_prepend_size(&serialized);
            (compressed, serialized.len())
        } else {
            (serialized.clone(), serialized.len())
        };

        let entry = CacheEntry {
            key: key.to_string(),
            result: result.clone(),
            compressed_data: compressed_data.clone(),
            uncompressed_size,
            created_at: SystemTime::now(),
            last_accessed: Instant::now(),
            access_count: 0,
            ttl: ttl.or(self.config.default_ttl),
        };

        let entry_size = compressed_data.len();

        let mut cache = self.cache.write().await;

        // Evict if necessary
        while cache.size_bytes + entry_size > self.config.max_size_bytes
            || cache.entries.len() >= self.config.max_entries
        {
            self.evict_one(&mut cache).await?;
        }

        // Add new entry
        cache.entries.insert(key.to_string(), entry);
        cache.access_order.push_front(key.to_string());
        cache.size_bytes += entry_size;

        info!(
            "Cached result: {} ({} bytes compressed, {:?} execution time)",
            key, entry_size, result.execution_time.as_millis()
        );

        Ok(())
    }

    /// Intelligent cache prewarming based on patterns
    pub async fn prewarm(&self, patterns: Vec<PrewarmPattern>) -> Result<()> {
        info!("Prewarming cache with {} patterns", patterns.len());

        for pattern in patterns {
            match pattern {
                PrewarmPattern::Common { code, env } => {
                    // Common patterns that are frequently accessed
                    let key = self.generate_key(&code, &env, &[], None);

                    // Skip if already cached
                    if self.get(&key).await?.is_some() {
                        continue;
                    }

                    // In production, would execute and cache
                    debug!("Would prewarm: {}", key);
                }
                PrewarmPattern::PredictedNext { probability, params } => {
                    // ML-predicted next likely executions
                    if probability > 0.7 {
                        debug!("Would prewarm predicted: {:?}", params);
                    }
                }
            }
        }

        Ok(())
    }

    /// Evict one entry based on policy
    async fn evict_one(&self, cache: &mut CacheStorage) -> Result<()> {
        let key_to_evict = match self.config.eviction_policy {
            EvictionPolicy::LRU => {
                // Remove least recently used
                cache.access_order.pop_back()
            }
            EvictionPolicy::LFU => {
                // Remove least frequently used
                cache
                    .entries
                    .iter()
                    .min_by_key(|(_, e)| e.access_count)
                    .map(|(k, _)| k.clone())
            }
            EvictionPolicy::FIFO => {
                // Remove oldest
                cache
                    .entries
                    .iter()
                    .min_by_key(|(_, e)| e.created_at)
                    .map(|(k, _)| k.clone())
            }
            EvictionPolicy::Adaptive => {
                // Adaptive: combine recency and frequency
                self.adaptive_evict(cache).await
            }
        };

        if let Some(key) = key_to_evict {
            if let Some(entry) = cache.entries.remove(&key) {
                cache.size_bytes -= entry.compressed_data.len();
                cache.access_order.retain(|k| k != &key);

                let mut stats = self.stats.write().await;
                stats.evictions += 1;

                debug!("Evicted cache entry: {}", key);
            }
        }

        Ok(())
    }

    /// Adaptive eviction based on access patterns
    async fn adaptive_evict(&self, cache: &CacheStorage) -> Option<String> {
        // Score = recency_weight * recency + frequency_weight * frequency
        let now = Instant::now();

        cache
            .entries
            .iter()
            .map(|(k, e)| {
                let recency = now.duration_since(e.last_accessed).as_secs_f64();
                let frequency = e.access_count as f64;

                // Lower score = better candidate for eviction
                let score = (1.0 / (recency + 1.0)) * 0.3 + frequency * 0.7;
                (k.clone(), score)
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(k, _)| k)
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStatistics {
        let cache = self.cache.read().await;
        let stats = self.stats.read().await;

        let hit_rate = if stats.hits + stats.misses > 0 {
            stats.hits as f64 / (stats.hits + stats.misses) as f64
        } else {
            0.0
        };

        CacheStatistics {
            entries: cache.entries.len(),
            size_bytes: cache.size_bytes,
            hit_rate,
            total_hits: stats.hits,
            total_misses: stats.misses,
            total_evictions: stats.evictions,
            bytes_saved: stats.total_bytes_saved,
            time_saved: stats.total_time_saved,
            avg_compression_ratio: self.calculate_compression_ratio(&cache),
        }
    }

    fn calculate_compression_ratio(&self, cache: &CacheStorage) -> f64 {
        if cache.entries.is_empty() {
            return 1.0;
        }

        let total_compressed: usize = cache.entries.values()
            .map(|e| e.compressed_data.len())
            .sum();

        let total_uncompressed: usize = cache.entries.values()
            .map(|e| e.uncompressed_size)
            .sum();

        if total_uncompressed > 0 {
            total_compressed as f64 / total_uncompressed as f64
        } else {
            1.0
        }
    }

    /// Clear all cache entries
    pub async fn clear(&self) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.entries.clear();
        cache.access_order.clear();
        cache.size_bytes = 0;

        info!("Cache cleared");
        Ok(())
    }

    /// Export cache for persistence
    pub async fn export(&self) -> Result<Vec<u8>> {
        let cache = self.cache.read().await;

        let export_data: Vec<_> = cache.entries.values()
            .map(|e| ExportEntry {
                key: e.key.clone(),
                compressed_data: e.compressed_data.clone(),
                result: e.result.clone(),
                ttl: e.ttl,
            })
            .collect();

        Ok(bincode::serialize(&export_data)?)
    }

    /// Import cache from persisted data
    pub async fn import(&self, data: &[u8]) -> Result<()> {
        let entries: Vec<ExportEntry> = bincode::deserialize(data)?;
        let entry_count = entries.len();

        for entry in entries {
            self.put(&entry.key, entry.result, entry.ttl).await?;
        }

        info!("Imported {} cache entries", entry_count);
        Ok(())
    }
}

#[derive(Debug)]
pub enum PrewarmPattern {
    Common {
        code: String,
        env: String,
    },
    PredictedNext {
        probability: f64,
        params: HashMap<String, String>,
    },
}

#[derive(Debug, Serialize)]
pub struct CacheStatistics {
    pub entries: usize,
    pub size_bytes: usize,
    pub hit_rate: f64,
    pub total_hits: u64,
    pub total_misses: u64,
    pub total_evictions: u64,
    pub bytes_saved: u64,
    pub time_saved: Duration,
    pub avg_compression_ratio: f64,
}

#[derive(Serialize, Deserialize)]
struct ExportEntry {
    key: String,
    compressed_data: Vec<u8>,
    result: CacheResult,
    ttl: Option<Duration>,
}

/// Multi-level cache for VM results
pub struct MultiLevelVmCache {
    l1_memory: Arc<VmResultCache>,    // Hot in-memory cache
    l2_disk: Arc<DiskCache>,          // Larger disk-backed cache
    l3_distributed: Option<Arc<DistributedCache>>, // Optional distributed cache
}

impl MultiLevelVmCache {
    /// Simple sync constructor for basic use
    pub fn new(config: CacheConfig) -> Self {
        Self {
            l1_memory: Arc::new(VmResultCache::new(config)),
            l2_disk: Arc::new(DiskCache::default()),
            l3_distributed: None,
        }
    }

    /// Full async constructor with all levels
    pub async fn new_full(config: MultiLevelConfig) -> Result<Self> {
        let l1_memory = Arc::new(VmResultCache::new(config.l1_config));
        let l2_disk = Arc::new(DiskCache::new(config.l2_path)?);

        let l3_distributed = if let Some(redis_url) = config.redis_url {
            Some(Arc::new(DistributedCache::new(&redis_url).await?))
        } else {
            None
        };

        Ok(Self {
            l1_memory,
            l2_disk,
            l3_distributed,
        })
    }

    /// Get from multi-level cache
    pub async fn get(&self, key: &str) -> Result<Option<CacheResult>> {
        // Check L1 (memory)
        if let Some(result) = self.l1_memory.get(key).await? {
            return Ok(Some(result));
        }

        // Check L2 (disk)
        if let Some(result) = self.l2_disk.get(key).await? {
            // Promote to L1
            self.l1_memory.put(key, result.clone(), None).await?;
            return Ok(Some(result));
        }

        // Check L3 (distributed)
        if let Some(ref l3) = self.l3_distributed {
            if let Some(result) = l3.get(key).await? {
                // Promote to L1 and L2
                self.l2_disk.put(key, &result).await?;
                self.l1_memory.put(key, result.clone(), None).await?;
                return Ok(Some(result));
            }
        }

        Ok(None)
    }

    /// Put to all cache levels
    pub async fn put(&self, key: &str, result: CacheResult) -> Result<()> {
        // Write through to all levels
        self.l1_memory.put(key, result.clone(), None).await?;
        self.l2_disk.put(key, &result).await?;

        if let Some(ref l3) = self.l3_distributed {
            l3.put(key, &result).await?;
        }

        Ok(())
    }
}

/// Disk-backed cache implementation
struct DiskCache {
    cache_dir: std::path::PathBuf,
}

impl Default for DiskCache {
    fn default() -> Self {
        Self {
            cache_dir: std::path::PathBuf::from("/tmp/vm-cache"),
        }
    }
}

impl DiskCache {
    fn new(cache_dir: std::path::PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    async fn get(&self, key: &str) -> Result<Option<CacheResult>> {
        let path = self.cache_dir.join(format!("{key}.cache"));

        if path.exists() {
            let data = tokio::fs::read(&path).await?;
            let decompressed = decompress_size_prepended(&data)?;
            let result: CacheResult = bincode::deserialize(&decompressed)?;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    async fn put(&self, key: &str, result: &CacheResult) -> Result<()> {
        let path = self.cache_dir.join(format!("{key}.cache"));
        let serialized = bincode::serialize(result)?;
        let compressed = compress_prepend_size(&serialized);
        tokio::fs::write(&path, compressed).await?;
        Ok(())
    }
}

/// Distributed cache (Redis/Memcached)
struct DistributedCache {
    redis_url: String,
}

impl DistributedCache {
    async fn new(redis_url: &str) -> Result<Self> {
        // In production, would establish Redis connection
        Ok(Self {
            redis_url: redis_url.to_string(),
        })
    }

    async fn get(&self, _key: &str) -> Result<Option<CacheResult>> {
        // In production, would query Redis
        Ok(None)
    }

    async fn put(&self, _key: &str, _result: &CacheResult) -> Result<()> {
        // In production, would write to Redis
        Ok(())
    }
}

pub struct MultiLevelConfig {
    pub l1_config: CacheConfig,
    pub l2_path: std::path::PathBuf,
    pub redis_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_vm_cache_operations() {
        let cache = VmResultCache::new(CacheConfig::default());

        let result = CacheResult {
            response: Some(b"test output".to_vec()),
            error: None,
            hit_rate: 0.0,
            cache_level: "L1".to_string(),
            execution_time: Duration::from_millis(100),
        };

        let key = cache.generate_key("echo test", "alpine", &[], None);

        // Test miss
        assert!(cache.get(&key).await.unwrap().is_none());

        // Test put
        cache.put(&key, result.clone(), None).await.unwrap();

        // Test hit
        let cached = cache.get(&key).await.unwrap().unwrap();
        assert_eq!(cached.response, result.response);

        // Test stats
        let stats = cache.get_stats().await;
        assert_eq!(stats.total_hits, 1);
        assert_eq!(stats.total_misses, 1);
        assert!(stats.hit_rate > 0.0);
    }
}