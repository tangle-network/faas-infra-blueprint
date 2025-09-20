
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// High-performance container pool with predictive warming and intelligent scaling
#[derive(Clone)]
pub struct ContainerPool {
    pools: Arc<RwLock<HashMap<String, EnvironmentPool>>>,
    metrics: Arc<RwLock<PoolMetrics>>,
    config: PoolConfig,
}

#[derive(Debug, Clone)]
pub struct EnvironmentPool {
    warm_containers: VecDeque<WarmContainer>,
    max_size: usize,
    min_size: usize,
    last_used: Instant,
    hit_rate: f64,
    avg_startup_time: Duration,
}

#[derive(Debug, Clone)]
pub struct WarmContainer {
    pub container_id: String,
    pub created_at: Instant,
    pub last_used: Instant,
    pub use_count: u32,
    pub environment: String,
    pub resource_usage: ResourceUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_mb: u64,
    pub cpu_percent: f64,
    pub startup_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_idle_time: Duration,
    pub max_containers_per_env: usize,
    pub min_containers_per_env: usize,
    pub predictive_warming: bool,
    pub cleanup_interval: Duration,
}

#[derive(Debug, Default, Clone)]
pub struct PoolMetrics {
    pub total_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub avg_acquisition_time: Duration,
    pub containers_created: u64,
    pub containers_destroyed: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_idle_time: Duration::from_secs(300), // 5 minutes
            max_containers_per_env: 10,
            min_containers_per_env: 1,
            predictive_warming: true,
            cleanup_interval: Duration::from_secs(60),
        }
    }
}

impl ContainerPool {
    pub fn new(config: PoolConfig) -> Self {
        let pool = Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(PoolMetrics::default())),
            config,
        };

        // Start background cleanup task
        let cleanup_pool = pool.clone();
        tokio::spawn(async move {
            cleanup_pool.run_cleanup_loop().await;
        });

        pool
    }

    /// Acquire a warm container or create a new one with sub-50ms target
    pub async fn acquire(&self, environment: &str) -> Result<WarmContainer> {
        let start = Instant::now();

        // Try to get from warm pool first
        if let Some(container) = self.try_acquire_warm(environment).await? {
            self.record_cache_hit(start.elapsed()).await;
            return Ok(container);
        }

        // Create new container if no warm available
        let container = self.create_new_container(environment).await?;
        self.record_cache_miss(start.elapsed()).await;

        Ok(container)
    }

    /// Return container to pool for reuse
    pub async fn release(&self, mut container: WarmContainer) -> Result<()> {
        container.last_used = Instant::now();
        container.use_count += 1;

        let mut pools = self.pools.write().await;
        let env_pool = pools
            .entry(container.environment.clone())
            .or_insert_with(|| EnvironmentPool::new(&self.config));

        // Only return to pool if under capacity and container is healthy
        if env_pool.warm_containers.len() < env_pool.max_size
            && self.is_container_healthy(&container).await?
        {
            env_pool.warm_containers.push_back(container);
        } else {
            // Destroy excess or unhealthy containers
            self.destroy_container(&container.container_id).await?;
        }

        Ok(())
    }

    /// Predictively warm containers based on usage patterns
    pub async fn predictive_warm(&self, predictions: Vec<(String, u32)>) -> Result<()> {
        if !self.config.predictive_warming {
            return Ok(());
        }

        let mut tasks = Vec::new();

        for (environment, count) in predictions {
            let pool = self.clone();
            tasks.push(tokio::spawn(async move {
                pool.ensure_warm_containers(&environment, count as usize)
                    .await
            }));
        }

        // Execute all warming tasks in parallel
        futures::future::try_join_all(tasks).await?;
        Ok(())
    }

    /// Ensure minimum number of warm containers for environment
    async fn ensure_warm_containers(&self, environment: &str, target_count: usize) -> Result<()> {
        let current_count = {
            let pools = self.pools.read().await;
            pools
                .get(environment)
                .map(|p| p.warm_containers.len())
                .unwrap_or(0)
        };

        if current_count < target_count {
            let needed = target_count - current_count;
            let mut tasks = Vec::new();

            for _ in 0..needed {
                let env = environment.to_string();
                let pool = self.clone();
                tasks.push(tokio::spawn(async move {
                    pool.create_new_container(&env).await
                }));
            }

            let containers = futures::future::try_join_all(tasks).await?;

            // Add to warm pool
            let mut pools = self.pools.write().await;
            let env_pool = pools
                .entry(environment.to_string())
                .or_insert_with(|| EnvironmentPool::new(&self.config));

            for container in containers {
                if env_pool.warm_containers.len() < env_pool.max_size {
                    env_pool.warm_containers.push_back(container?);
                }
            }
        }

        Ok(())
    }

    async fn try_acquire_warm(&self, environment: &str) -> Result<Option<WarmContainer>> {
        let mut pools = self.pools.write().await;

        if let Some(env_pool) = pools.get_mut(environment) {
            // Remove stale containers first
            env_pool
                .warm_containers
                .retain(|c| c.created_at.elapsed() < self.config.max_idle_time);

            // Get the most recently used container (LIFO for better cache locality)
            if let Some(container) = env_pool.warm_containers.pop_back() {
                env_pool.last_used = Instant::now();
                return Ok(Some(container));
            }
        }

        Ok(None)
    }

    async fn create_new_container(&self, environment: &str) -> Result<WarmContainer> {
        let start = Instant::now();

        // TODO: Replace with actual container creation logic
        // This would integrate with Docker/Firecracker
        let container_id = format!("container-{}-{}", environment, uuid::Uuid::new_v4());

        // Simulate container creation time (in production, this would be actual Docker API calls)
        tokio::time::sleep(Duration::from_millis(50)).await;

        let creation_time = start.elapsed();

        let container = WarmContainer {
            container_id,
            created_at: Instant::now(),
            last_used: Instant::now(),
            use_count: 0,
            environment: environment.to_string(),
            resource_usage: ResourceUsage {
                memory_mb: 128,
                cpu_percent: 5.0,
                startup_time_ms: creation_time.as_millis() as u64,
            },
        };

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.containers_created += 1;

        Ok(container)
    }

    async fn is_container_healthy(&self, container: &WarmContainer) -> Result<bool> {
        // Check if container is still running and responsive
        // In production, this would check container status via Docker API

        // Basic health checks
        let age = container.created_at.elapsed();
        let idle_time = container.last_used.elapsed();

        Ok(age < Duration::from_secs(3600)
            && idle_time < self.config.max_idle_time
            && container.use_count < 1000) // Prevent infinite reuse
    }

    async fn destroy_container(&self, container_id: &str) -> Result<()> {
        // TODO: Implement actual container destruction
        // This would call Docker API to stop and remove container
        tracing::debug!("Destroying container: {}", container_id);

        let mut metrics = self.metrics.write().await;
        metrics.containers_destroyed += 1;

        Ok(())
    }

    async fn record_cache_hit(&self, acquisition_time: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;
        metrics.cache_hits += 1;
        metrics.avg_acquisition_time = (metrics.avg_acquisition_time + acquisition_time) / 2;
    }

    async fn record_cache_miss(&self, acquisition_time: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;
        metrics.cache_misses += 1;
        metrics.avg_acquisition_time = (metrics.avg_acquisition_time + acquisition_time) / 2;
    }

    async fn run_cleanup_loop(&self) {
        let mut interval = tokio::time::interval(self.config.cleanup_interval);

        loop {
            interval.tick().await;
            if let Err(e) = self.cleanup_expired_containers().await {
                tracing::error!("Container cleanup failed: {}", e);
            }
        }
    }

    async fn cleanup_expired_containers(&self) -> Result<()> {
        let mut pools = self.pools.write().await;
        let mut total_cleaned = 0;

        for (env_name, env_pool) in pools.iter_mut() {
            let initial_count = env_pool.warm_containers.len();

            // Remove expired containers
            env_pool.warm_containers.retain(|container| {
                let expired = container.last_used.elapsed() > self.config.max_idle_time;
                if expired {
                    // Note: In production, we'd need to actually destroy the container
                    tracing::debug!("Cleaning up expired container: {}", container.container_id);
                }
                !expired
            });

            let cleaned = initial_count - env_pool.warm_containers.len();
            total_cleaned += cleaned;

            // Update pool statistics
            if env_pool.warm_containers.is_empty() {
                env_pool.last_used = Instant::now();
            }
        }

        // Remove empty pools
        pools.retain(|_, pool| {
            !pool.warm_containers.is_empty() || pool.last_used.elapsed() < Duration::from_secs(3600)
        });

        if total_cleaned > 0 {
            tracing::info!("Cleaned up {} expired containers", total_cleaned);
        }

        Ok(())
    }

    /// Get current pool metrics for monitoring
    pub async fn get_metrics(&self) -> PoolMetrics {
        (*self.metrics.read().await).clone()
    }

    /// Get detailed pool status for debugging
    pub async fn get_pool_status(&self) -> HashMap<String, PoolStatus> {
        let pools = self.pools.read().await;
        let mut status = HashMap::new();

        for (env_name, env_pool) in pools.iter() {
            status.insert(
                env_name.clone(),
                PoolStatus {
                    warm_count: env_pool.warm_containers.len(),
                    max_size: env_pool.max_size,
                    min_size: env_pool.min_size,
                    hit_rate: env_pool.hit_rate,
                    avg_startup_time: env_pool.avg_startup_time,
                    last_used: env_pool.last_used,
                },
            );
        }

        status
    }
}

impl EnvironmentPool {
    fn new(config: &PoolConfig) -> Self {
        Self {
            warm_containers: VecDeque::new(),
            max_size: config.max_containers_per_env,
            min_size: config.min_containers_per_env,
            last_used: Instant::now(),
            hit_rate: 0.0,
            avg_startup_time: Duration::from_millis(100),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub warm_count: usize,
    pub max_size: usize,
    pub min_size: usize,
    pub hit_rate: f64,
    pub avg_startup_time: Duration,
    pub last_used: Instant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_container_pool_acquire_release() {
        let pool = ContainerPool::new(PoolConfig::default());

        // Acquire container (should create new)
        let container = pool.acquire("alpine:latest").await.unwrap();
        assert_eq!(container.environment, "alpine:latest");

        // Release back to pool
        pool.release(container.clone()).await.unwrap();

        // Acquire again (should reuse)
        let container2 = pool.acquire("alpine:latest").await.unwrap();
        assert_eq!(container.container_id, container2.container_id);
    }

    #[tokio::test]
    async fn test_predictive_warming() {
        let pool = ContainerPool::new(PoolConfig::default());

        // Warm containers predictively
        pool.predictive_warm(vec![
            ("python:3".to_string(), 2),
            ("node:18".to_string(), 1),
        ])
        .await
        .unwrap();

        let status = pool.get_pool_status().await;
        assert_eq!(status.get("python:3").unwrap().warm_count, 2);
        assert_eq!(status.get("node:18").unwrap().warm_count, 1);
    }

    #[tokio::test]
    async fn test_metrics_tracking() {
        let pool = ContainerPool::new(PoolConfig::default());

        // Generate some activity
        let container = pool.acquire("alpine:latest").await.unwrap();
        pool.release(container).await.unwrap();

        let _container2 = pool.acquire("alpine:latest").await.unwrap();

        let metrics = pool.get_metrics().await;
        assert_eq!(metrics.total_requests, 2);
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_misses, 1);
    }
}
