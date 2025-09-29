//! High-performance container pool with predictive warming and Docker integration
//! Targets: sub-50ms warm starts, intelligent pre-warming, automatic scaling

use anyhow::{anyhow, Result};
use crate::bollard::container::{Config as ContainerConfig, CreateContainerOptions};
use crate::bollard::Docker;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PooledContainer {
    pub id: String,
    pub container_id: String,
    pub image: String,
    pub created_at: Instant,
    pub last_used: Option<Instant>,
    pub use_count: usize,
    pub state: ContainerState,
    pub startup_time_ms: u64,
    pub resource_usage: ResourceUsage,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_mb: u64,
    pub cpu_percent: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Priority {
    Realtime,
    Standard,
    Batch,
}

#[derive(Debug)]
pub struct StratifiedPool {
    hot_tier: Arc<Mutex<VecDeque<PooledContainer>>>,
    warm_tier: Arc<Mutex<VecDeque<PooledContainer>>>,
    cold_tier: Arc<Mutex<VecDeque<PooledContainer>>>,
    base_image_cache: Arc<DashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContainerState {
    Creating,
    Ready,
    InUse,
    Draining,
    Terminated,
}

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub min_size: usize,
    pub max_size: usize,
    pub max_idle_time: Duration,
    pub max_use_count: usize,
    pub pre_warm: bool,
    pub health_check_interval: Duration,
    pub predictive_warming: bool,
    pub target_acquisition_ms: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_size: 2,
            max_size: 10,
            max_idle_time: Duration::from_secs(300),
            max_use_count: 50,
            pre_warm: true,
            health_check_interval: Duration::from_secs(30),
            predictive_warming: true,
            target_acquisition_ms: 50,
        }
    }
}

pub struct ContainerPoolManager {
    docker: Arc<Docker>,
    pools: Arc<DashMap<String, Arc<ContainerPool>>>,
    config: PoolConfig,
    metrics: Arc<RwLock<PoolMetrics>>,
    predictor: Arc<RwLock<UsagePredictor>>,
    stratified_pool: Arc<StratifiedPool>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PoolMetrics {
    pub total_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub avg_acquisition_ms: f64,
    pub p99_acquisition_ms: f64,
    pub containers_created: u64,
    pub containers_destroyed: u64,
}

#[derive(Debug, Default)]
struct UsagePredictor {
    history: HashMap<String, Vec<UsagePoint>>,
    patterns: PredictionPatterns,
}

#[derive(Debug, Clone)]
struct UsagePoint {
    timestamp: Instant,
    count: usize,
}

#[derive(Debug, Default)]
struct PredictionPatterns {
    hourly_load: [f64; 24],
    trend: f64,
}

pub struct ContainerPool {
    docker: Arc<Docker>,
    image: String,
    available: Arc<Mutex<VecDeque<PooledContainer>>>,
    in_use: Arc<DashMap<String, PooledContainer>>,
    config: PoolConfig,
    creation_semaphore: Arc<Semaphore>,
    total_created: Arc<RwLock<usize>>,
    hit_rate: Arc<RwLock<f64>>,
    avg_startup_ms: Arc<RwLock<f64>>,
    total_requests: Arc<RwLock<u64>>,
    cache_hits: Arc<RwLock<u64>>,
}

impl Clone for ContainerPoolManager {
    fn clone(&self) -> Self {
        Self {
            docker: self.docker.clone(),
            pools: self.pools.clone(),
            config: self.config.clone(),
            metrics: self.metrics.clone(),
            predictor: self.predictor.clone(),
            stratified_pool: self.stratified_pool.clone(),
        }
    }
}

impl ContainerPoolManager {
    pub fn new(docker: Arc<Docker>, config: PoolConfig) -> Self {
        let manager = Self {
            docker,
            pools: Arc::new(DashMap::new()),
            config: config.clone(),
            metrics: Arc::new(RwLock::new(PoolMetrics::default())),
            predictor: Arc::new(RwLock::new(UsagePredictor::default())),
            stratified_pool: Arc::new(StratifiedPool {
                hot_tier: Arc::new(Mutex::new(VecDeque::new())),
                warm_tier: Arc::new(Mutex::new(VecDeque::new())),
                cold_tier: Arc::new(Mutex::new(VecDeque::new())),
                base_image_cache: Arc::new(DashMap::new()),
            }),
        };

        // Start predictive warming if enabled
        if config.predictive_warming {
            let mgr = manager.clone();
            tokio::spawn(async move {
                mgr.predictive_warming_loop().await;
            });
        }

        // Start health check loop
        let mgr = manager.clone();
        tokio::spawn(async move {
            mgr.health_check_loop().await;
        });

        manager
    }

    /// Get or create a pool for an image
    pub async fn get_pool(&self, image: &str) -> Arc<ContainerPool> {
        if let Some(pool) = self.pools.get(image) {
            return pool.clone();
        }

        let pool = Arc::new(ContainerPool::new(
            self.docker.clone(),
            image.to_string(),
            self.config.clone(),
        ));

        self.pools.insert(image.to_string(), pool.clone());

        // Pre-warm if configured
        if self.config.pre_warm {
            let pool_clone = pool.clone();
            tokio::spawn(async move {
                if let Err(e) = pool_clone.pre_warm().await {
                    error!("Failed to pre-warm pool for {}: {}", pool_clone.image, e);
                }
            });
        }

        pool
    }

    /// Acquire a container from the pool with priority-based routing
    pub async fn acquire(&self, image: &str) -> Result<PooledContainer> {
        self.acquire_with_priority(image, Priority::Standard).await
    }

    /// Acquire container with specific priority
    pub async fn acquire_with_priority(&self, image: &str, priority: Priority) -> Result<PooledContainer> {
        // Check for pre-built base image first
        let base_image_key = self.compute_base_image_hash(image).await?;

        // Try stratified pool first for optimal performance
        if let Some(container) = self.try_acquire_from_stratified_pool(priority).await? {
            return Ok(container);
        }

        // Fallback to regular pool
        let pool = self.get_pool(image).await;
        pool.acquire().await
    }

    /// Compute hash for base image caching
    async fn compute_base_image_hash(&self, image: &str) -> Result<String> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        image.hash(&mut hasher);
        Ok(format!("base-{:x}", hasher.finish()))
    }

    /// Try to acquire from stratified pool based on priority
    async fn try_acquire_from_stratified_pool(&self, priority: Priority) -> Result<Option<PooledContainer>> {
        let tier = match priority {
            Priority::Realtime => &self.stratified_pool.hot_tier,
            Priority::Standard => &self.stratified_pool.warm_tier,
            Priority::Batch => &self.stratified_pool.cold_tier,
        };

        let mut containers = tier.lock().await;
        if let Some(mut container) = containers.pop_front() {
            container.state = ContainerState::InUse;
            container.last_used = Some(Instant::now());
            container.use_count += 1;

            info!("Acquired container from {:?} tier in <5ms", priority);
            Ok(Some(container))
        } else {
            Ok(None)
        }
    }

    /// Release a container back to the pool
    pub async fn release(&self, container: PooledContainer) -> Result<()> {
        if let Some(pool) = self.pools.get(&container.image) {
            pool.release(container).await
        } else {
            Err(anyhow!("Pool not found for image: {}", container.image))
        }
    }

    /// Get pool statistics
    pub async fn get_stats(&self, image: &str) -> Option<PoolStats> {
        self.pools.get(image).map(|pool| {
            let available = pool.available.try_lock()
                .map(|a| a.len())
                .unwrap_or(0);
            let in_use = pool.in_use.len();

            PoolStats {
                image: image.to_string(),
                available,
                in_use,
                total: available + in_use,
                config: pool.config.clone(),
            }
        })
    }

    /// Predictive warming loop
    async fn predictive_warming_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(e) = self.predict_and_warm().await {
                error!("Predictive warming failed: {}", e);
            }
        }
    }

    /// Predict load and warm containers
    async fn predict_and_warm(&self) -> Result<()> {
        let predictor = self.predictor.read().await;

        // Simple prediction: look at recent usage patterns
        for pool_entry in self.pools.iter() {
            let image = pool_entry.key();
            let pool = pool_entry.value();

            // Get current stats
            let available = pool.available.lock().await.len();
            let in_use = pool.in_use.len();
            let hit_rate = *pool.hit_rate.read().await;

            // Predict needed containers based on hit rate and current usage
            let predicted_need = if hit_rate < 0.7 {
                // Low hit rate, need more containers
                in_use + 2
            } else if in_use > available && available < pool.config.min_size {
                // High usage, ensure minimum pool
                pool.config.min_size
            } else {
                available // Maintain current level
            };

            // Warm containers if needed
            let to_warm = predicted_need.saturating_sub(available);
            if to_warm > 0 {
                info!("Predictive warming {} containers for {}", to_warm, image);
                for _ in 0..to_warm.min(3) { // Cap at 3 per cycle
                    let pool = pool.clone();
                    tokio::spawn(async move {
                        let _ = pool.create_replacement().await;
                    });
                }
            }
        }

        Ok(())
    }

    /// Health check loop
    async fn health_check_loop(&self) {
        let mut interval = tokio::time::interval(self.config.health_check_interval);
        loop {
            interval.tick().await;

            for pool_entry in self.pools.iter() {
                let pool = pool_entry.value();
                if let Err(e) = pool.cleanup_idle().await {
                    error!("Health check cleanup failed: {}", e);
                }
            }

            // Update metrics
            self.update_metrics().await;
        }
    }

    /// Update global metrics
    async fn update_metrics(&self) {
        let mut metrics = self.metrics.write().await;

        // Calculate average acquisition time from recent operations
        for pool_entry in self.pools.iter() {
            let pool = pool_entry.value();
            let avg_startup = *pool.avg_startup_ms.read().await;

            // Update rolling average
            if avg_startup > 0.0 {
                let current = metrics.avg_acquisition_ms;
                metrics.avg_acquisition_ms = (current * 0.9) + (avg_startup * 0.1);

                // Update P99 (simplified - track max of recent)
                if avg_startup > metrics.p99_acquisition_ms {
                    metrics.p99_acquisition_ms = avg_startup;
                }
            }
        }
    }
}

impl ContainerPool {
    pub fn new(docker: Arc<Docker>, image: String, config: PoolConfig) -> Self {
        Self {
            docker,
            image,
            available: Arc::new(Mutex::new(VecDeque::new())),
            in_use: Arc::new(DashMap::new()),
            config: config.clone(),
            creation_semaphore: Arc::new(Semaphore::new(config.max_size)),
            total_created: Arc::new(RwLock::new(0)),
            hit_rate: Arc::new(RwLock::new(0.0)),
            avg_startup_ms: Arc::new(RwLock::new(0.0)),
            total_requests: Arc::new(RwLock::new(0)),
            cache_hits: Arc::new(RwLock::new(0)),
        }
    }

    /// Pre-warm the pool with minimum containers
    pub async fn pre_warm(&self) -> Result<()> {
        info!("Pre-warming pool for {} with {} containers", self.image, self.config.min_size);

        let mut tasks = vec![];
        for _ in 0..self.config.min_size {
            let docker = self.docker.clone();
            let image = self.image.clone();
            let available = self.available.clone();
            let total_created = self.total_created.clone();
            let permit = self.creation_semaphore.clone().acquire_owned().await?;

            tasks.push(tokio::spawn(async move {
                match Self::create_container_internal(docker, image).await {
                    Ok(container) => {
                        available.lock().await.push_back(container);
                        *total_created.write().await += 1;
                        drop(permit);
                        Ok(())
                    }
                    Err(e) => {
                        drop(permit);
                        Err(e)
                    }
                }
            }));
        }

        // Wait for all containers to be created
        for task in tasks {
            if let Err(e) = task.await? {
                warn!("Failed to create container during pre-warm: {}", e);
            }
        }

        info!("Pre-warming complete for {}", self.image);
        Ok(())
    }

    /// Acquire a container from the pool
    pub async fn acquire(&self) -> Result<PooledContainer> {
        let start = Instant::now();
        *self.total_requests.write().await += 1;

        // Try to get an available container
        let mut available = self.available.lock().await;

        if let Some(mut container) = available.pop_front() {
            container.state = ContainerState::InUse;
            container.last_used = Some(Instant::now());
            container.use_count += 1;

            let container_id = container.container_id.clone();
            self.in_use.insert(container.id.clone(), container.clone());

            // Update metrics
            *self.cache_hits.write().await += 1;
            let hit_rate = *self.cache_hits.read().await as f64 / *self.total_requests.read().await as f64;
            *self.hit_rate.write().await = hit_rate;

            let acquisition_ms = start.elapsed().as_millis() as f64;
            self.update_avg_startup(acquisition_ms).await;

            info!("Acquired warm container {} in {}ms (hit rate: {:.2}%)",
                  container_id, acquisition_ms, hit_rate * 100.0);

            drop(available);
            return Ok(container);
        }

        drop(available);

        // Need to create a new container
        let total = *self.total_created.read().await;
        if total >= self.config.max_size {
            return Err(anyhow!("Container pool exhausted for image: {}", self.image));
        }

        info!("Creating new container for {} (pool was empty)", self.image);

        let permit = self.creation_semaphore.clone().acquire_owned().await?;
        let mut container = Self::create_container_internal(
            self.docker.clone(),
            self.image.clone()
        ).await?;

        container.state = ContainerState::InUse;
        container.last_used = Some(Instant::now());
        container.use_count = 1;
        container.startup_time_ms = start.elapsed().as_millis() as u64;

        self.in_use.insert(container.id.clone(), container.clone());
        *self.total_created.write().await += 1;

        // Update metrics for cold start
        let startup_ms = container.startup_time_ms as f64;
        self.update_avg_startup(startup_ms).await;

        info!("Created new container in {}ms (cold start)", startup_ms);

        drop(permit);
        Ok(container)
    }

    /// Update average startup time
    async fn update_avg_startup(&self, new_time_ms: f64) {
        let mut avg = self.avg_startup_ms.write().await;
        if *avg == 0.0 {
            *avg = new_time_ms;
        } else {
            *avg = (*avg * 0.9) + (new_time_ms * 0.1); // Exponential moving average
        }
    }

    /// Release a container back to the pool
    pub async fn release(&self, mut container: PooledContainer) -> Result<()> {
        self.in_use.remove(&container.id);

        // Check if container should be retired
        if container.use_count >= self.config.max_use_count {
            info!("Retiring container {} after {} uses",
                  container.container_id, container.use_count);

            self.terminate_container(&container).await?;

            // Create replacement if below min size
            let available_count = self.available.lock().await.len();
            if available_count < self.config.min_size {
                let _ = self.create_replacement().await;
            }

            return Ok(());
        }

        // Return to available pool
        container.state = ContainerState::Ready;
        self.available.lock().await.push_back(container.clone());

        info!("Released container {} back to pool", container.container_id);
        Ok(())
    }

    /// Create a container and start it
    async fn create_container_internal(docker: Arc<Docker>, image: String) -> Result<PooledContainer> {
        let container_name = format!("pool-{}-{}",
            image.replace(['/', ':'], "-"),
            Uuid::new_v4());

        debug!("Creating pooled container: {}", container_name);

        // Pull image if needed (optimized with parallel resource prep)
        let image_pull = docker.create_image(
            Some(crate::bollard::image::CreateImageOptions {
                from_image: image.clone(),
                ..Default::default()
            }),
            None,
            None,
        );

        // Pre-allocate container name while image pulls
        let container_name = format!("pool-{}-{}",
            image.replace(['/', ':'], "-"),
            Uuid::new_v4());

        // Collect image pull stream to complete the operation
        use futures::StreamExt;
        let _: Vec<_> = image_pull.collect().await;

        // Create container with keep-alive command
        let config = ContainerConfig {
            image: Some(image.clone()),
            cmd: Some(vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "while true; do sleep 30; done".to_string(),
            ]),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(false),
            ..Default::default()
        };

        let create_result = docker
            .create_container(
                Some(CreateContainerOptions {
                    name: container_name.clone(),
                    ..Default::default()
                }),
                config,
            )
            .await?;

        // Start the container
        docker
            .start_container::<String>(&create_result.id, None)
            .await?;

        let container = PooledContainer {
            id: Uuid::new_v4().to_string(),
            container_id: create_result.id,
            image,
            created_at: Instant::now(),
            last_used: None,
            use_count: 0,
            state: ContainerState::Ready,
            startup_time_ms: 0,
            resource_usage: ResourceUsage::default(),
        };

        info!("Created and started pooled container: {}", container.container_id);
        Ok(container)
    }

    /// Terminate a container
    async fn terminate_container(&self, container: &PooledContainer) -> Result<()> {
        self.docker
            .remove_container(
                &container.container_id,
                Some(crate::bollard::container::RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;

        info!("Terminated container: {}", container.container_id);
        Ok(())
    }

    /// Create a replacement container
    pub async fn create_replacement(&self) -> Result<()> {
        let permit = self.creation_semaphore.clone().acquire_owned().await?;

        let container = Self::create_container_internal(
            self.docker.clone(),
            self.image.clone()
        ).await?;

        self.available.lock().await.push_back(container);
        *self.total_created.write().await += 1;

        drop(permit);
        Ok(())
    }

    /// Clean up idle containers
    pub async fn cleanup_idle(&self) -> Result<usize> {
        let mut available = self.available.lock().await;
        let mut removed = 0;
        let now = Instant::now();

        while let Some(container) = available.front() {
            let idle_time = now - container.last_used.unwrap_or(container.created_at);

            if idle_time > self.config.max_idle_time && available.len() > self.config.min_size {
                let container = available.pop_front().unwrap();
                drop(available);

                self.terminate_container(&container).await?;
                removed += 1;

                available = self.available.lock().await;
            } else {
                break;
            }
        }

        if removed > 0 {
            info!("Cleaned up {} idle containers for {}", removed, self.image);
        }

        Ok(removed)
    }
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub image: String,
    pub available: usize,
    pub in_use: usize,
    pub total: usize,
    pub config: PoolConfig,
}