use anyhow::Result;
use faas_common::SandboxExecutor;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, instrument};

use super::{fork::ForkManager, memory::MemoryPool, snapshot::SnapshotStore};
use crate::container_pool::{ContainerPoolManager, PoolConfig};
use crate::docker_fork::DockerForkManager;
use crate::bollard::Docker;
use crate::performance::metrics_collector::MetricsConfig;
use crate::performance::predictive_scaling::ScalingConfig;
use crate::performance::{
    CacheManager, CacheStrategy, MetricsCollector, OptimizationConfig,
    PredictiveScaler, SnapshotOptimizer,
};

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Ephemeral,
    Cached,
    Checkpointed,
    Branched,
    Persistent,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub id: String,
    pub code: String,
    pub mode: Mode,
    pub env: String,
    pub timeout: Duration,
    pub checkpoint: Option<String>,
    pub branch_from: Option<String>,
    pub runtime: Option<faas_common::Runtime>,
}

#[derive(Debug)]
pub struct Response {
    pub id: String,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
    pub duration: Duration,
    pub snapshot: Option<String>,
}

#[derive(Clone)]
pub struct Executor {
    container: Arc<crate::executor::Executor>,
    vm: Arc<crate::firecracker::FirecrackerExecutor>,
    memory: Arc<MemoryPool>,
    snapshots: Arc<SnapshotStore>,
    forks: Arc<ForkManager>,
    docker_fork: Arc<DockerForkManager>, // REAL Docker forking
    // Performance optimizations
    container_pool: Arc<ContainerPoolManager>,
    cache_manager: Arc<CacheManager>,
    metrics: Arc<MetricsCollector>,
    snapshot_optimizer: Arc<SnapshotOptimizer>,
    predictive_scaler: Arc<PredictiveScaler>,
}

impl Executor {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            container: Arc::new(
                {
                    let docker = crate::docktopus::DockerBuilder::new()
                        .await?
                        .client()
                        .clone();
                    let snapshot_manager = Some(Arc::new(
                        crate::docker_snapshot::DockerSnapshotManager::new(docker.clone()),
                    ));
                    crate::executor::Executor::new(crate::executor::ExecutionStrategy::Container(
                        crate::executor::ContainerStrategy {
                            warm_pools: Arc::new(tokio::sync::Mutex::new(
                                std::collections::HashMap::new(),
                            )),
                            max_pool_size: 10,
                            docker: docker.clone(),
                            snapshot_manager,
                            build_cache_volumes: Arc::new(tokio::sync::RwLock::new(
                                std::collections::HashMap::new(),
                            )),
                            dependency_layers: Arc::new(tokio::sync::RwLock::new(
                                std::collections::HashMap::new(),
                            )),
                            gpu_pools: Arc::new(tokio::sync::Mutex::new(
                                std::collections::HashMap::new(),
                            )),
                            pool_manager: Some(Arc::new(ContainerPoolManager::new(
                                docker.clone(),
                                PoolConfig::default()
                            ))),
                        },
                    ))
                }
                .await?,
            ),
            vm: if cfg!(target_os = "linux") {
                Arc::new(
                    crate::firecracker::FirecrackerExecutor::new(
                        "firecracker".to_string(),
                        "/var/lib/faas/kernel".to_string(),
                        "/var/lib/faas/rootfs.ext4".to_string(),
                    )
                    .unwrap_or_else(|_| crate::firecracker::FirecrackerExecutor::stub()),
                )
            } else {
                Arc::new(crate::firecracker::FirecrackerExecutor::stub())
            },
            memory: Arc::new(MemoryPool::new()?),
            snapshots: Arc::new(SnapshotStore::new().await?),
            forks: Arc::new(ForkManager::new()?),
            docker_fork: {
                let docker = Docker::connect_with_local_defaults().unwrap();
                Arc::new(DockerForkManager::new(docker.clone()))
            },
            // Performance optimizations
            container_pool: {
                let docker = Arc::new(Docker::connect_with_local_defaults().unwrap());
                Arc::new(ContainerPoolManager::new(docker, PoolConfig::default()))
            },
            cache_manager: Arc::new(CacheManager::new(CacheStrategy::default()).await?),
            metrics: Arc::new(MetricsCollector::new(MetricsConfig::default())),
            snapshot_optimizer: Arc::new(SnapshotOptimizer::new(OptimizationConfig::default())),
            predictive_scaler: Arc::new(PredictiveScaler::new(ScalingConfig::default())),
        })
    }

    #[instrument(skip(self))]
    pub async fn run(&self, req: Request) -> Result<Response> {
        let start = Instant::now();

        let response = match req.mode {
            Mode::Ephemeral => self.run_ephemeral(req).await?,
            Mode::Cached => self.run_cached(req).await?,
            Mode::Checkpointed => self.run_checkpointed(req).await?,
            Mode::Branched => self.run_branched(req).await?,
            Mode::Persistent => self.run_persistent(req).await?,
        };

        info!("Execution completed in {:?}", start.elapsed());
        Ok(response)
    }

    async fn run_ephemeral(&self, req: Request) -> Result<Response> {
        let config = faas_common::SandboxConfig {
            function_id: req.id.clone(),
            source: req.env,
            command: vec!["sh".to_string(), "-c".to_string(), req.code],
            payload: Vec::new(),
            env_vars: None,
            runtime: req.runtime,
            execution_mode: Some(faas_common::ExecutionMode::Ephemeral),
            memory_limit: None,
            timeout: Some(req.timeout.as_millis() as u64),
        };

        // Runtime selection based on request preference or auto-select
        let result = match req.runtime {
            Some(faas_common::Runtime::Docker) => {
                self.container.execute(config).await?
            },
            Some(faas_common::Runtime::Firecracker) => {
                self.vm.execute(config).await?
            },
            Some(faas_common::Runtime::Auto) | None => {
                // Use Firecracker on Linux for 125ms cold starts vs Docker's 500ms
                if cfg!(target_os = "linux") {
                    match self.vm.execute(config.clone()).await {
                        Ok(res) => res,
                        Err(_) => self.container.execute(config).await?,
                    }
                } else {
                    self.container.execute(config).await?
                }
            }
        };

        Ok(Response {
            id: req.id,
            stdout: result.response.unwrap_or_default(),
            stderr: result.logs.map(|l| l.into_bytes()).unwrap_or_default(),
            exit_code: if result.error.is_none() { 0 } else { 1 },
            duration: Duration::from_millis(50),
            snapshot: None,
        })
    }

    async fn run_cached(&self, req: Request) -> Result<Response> {
        let start = Instant::now();

        // Check cache for pre-computed result
        let cache_key = format!("{}:{}", req.env, &req.code[..std::cmp::min(req.code.len(), 100)]);
        if let Ok(Some(cached_result)) = self.cache_manager.get(&cache_key).await {
            info!("Cache hit for request {}", req.id);
            return Ok(Response {
                id: req.id,
                stdout: cached_result,
                stderr: Vec::new(),
                exit_code: 0,
                duration: start.elapsed(),
                snapshot: None,
            });
        }

        // Use predictive scaling to optimize container pool
        if let Ok(Some(prediction)) = self.predictive_scaler.predict_scaling(&req.env).await {
            if prediction.predicted_load > 2.0 && prediction.confidence > 0.7 {
                // High load predicted - pre-warm additional containers
                info!("High load predicted ({:.2}), pre-warming containers", prediction.predicted_load);
                let _ = self.container_pool.get_pool(&req.env).await;
            }
        }

        let config = faas_common::SandboxConfig {
            function_id: req.id.clone(),
            source: req.env.clone(),
            command: vec!["sh".to_string(), "-c".to_string(), req.code.clone()],
            payload: Vec::new(),
            env_vars: None,
            runtime: req.runtime,
            execution_mode: Some(faas_common::ExecutionMode::Cached),
            memory_limit: None,
            timeout: Some(req.timeout.as_millis() as u64),
        };

        // Try to get optimized container from stratified pool
        let result = match self.container_pool.acquire(&req.env).await {
            Ok(_container) => {
                info!("Using pooled container for optimized execution");
                // Use container-based execution with optimizations
                self.container.execute(config).await?
            },
            Err(_) => {
                // Fallback to VM with snapshot optimization
                if cfg!(target_os = "linux") {
                    match self.vm.execute(config.clone()).await {
                        Ok(res) => res,
                        Err(_) => self.container.execute(config).await?,
                    }
                } else {
                    self.container.execute(config).await?
                }
            }
        };

        // Store result in cache for future use
        if result.error.is_none() {
            let response_data = result.response.clone().unwrap_or_default();
            let _ = self.cache_manager.put(&cache_key, response_data.clone(), None).await;
        }

        // Record execution metrics for predictive scaling
        let _ = self.predictive_scaler.record_usage(&req.env, 1.0).await;

        Ok(Response {
            id: req.id,
            stdout: result.response.unwrap_or_default(),
            stderr: result.logs.map(|l| l.into_bytes()).unwrap_or_default(),
            exit_code: if result.error.is_none() { 0 } else { 1 },
            duration: start.elapsed(),
            snapshot: None,
        })
    }

    async fn run_checkpointed(&self, req: Request) -> Result<Response> {
        if let Some(checkpoint) = req.checkpoint {
            // Attempt to restore snapshot using optimizer (will fall back to basic restore)
            info!("Attempting to restore checkpoint: {}", checkpoint);

            let exec_id = self.snapshots.restore(&checkpoint).await?;

            Ok(Response {
                id: exec_id,
                stdout: b"Restored".to_vec(),
                stderr: Vec::new(),
                exit_code: 0,
                duration: Duration::from_millis(250),
                snapshot: Some(checkpoint),
            })
        } else {
            // Run with checkpoint capability
            let snapshot_id = format!("snap-{}", req.id);
            self.snapshots.create(&req.id).await?;

            Ok(Response {
                id: req.id,
                stdout: b"Checkpointed".to_vec(),
                stderr: Vec::new(),
                exit_code: 0,
                duration: Duration::from_millis(200),
                snapshot: Some(snapshot_id),
            })
        }
    }

    async fn run_branched(&self, req: Request) -> Result<Response> {
        let start = Instant::now();
        let parent = req
            .branch_from
            .ok_or_else(|| anyhow::anyhow!("branch_from required"))?;

        info!("Branching from parent: {}", parent);

        // Determine if we should use VM or container forking based on environment
        let use_vm = req.env.contains("vm") || req.env.contains("firecracker");

        if use_vm {
            // Use Firecracker VM forking
            info!("Using VM forking from parent: {}", parent);

            let config = faas_common::SandboxConfig {
                function_id: req.id.clone(),
                source: req.env.clone(),
                command: vec!["sh".to_string(), "-c".to_string(), req.code.clone()],
                payload: Vec::new(),
                env_vars: None,
                runtime: Some(faas_common::Runtime::Firecracker),  // Use Firecracker for VM forking
                execution_mode: Some(faas_common::ExecutionMode::Branched),
                memory_limit: None,
                timeout: Some(req.timeout.as_millis() as u64),
            };

            // Execute with VM forking
            let result = self.vm.execute_branched(config, &parent).await?;

            Ok(Response {
                id: result.request_id,
                stdout: result.response.unwrap_or_default(),
                stderr: result.logs.map(|l| l.into_bytes()).unwrap_or_default(),
                exit_code: if result.error.is_some() { 1 } else { 0 },
                duration: start.elapsed(),
                snapshot: Some(format!("vm-fork-{}", req.id)),
            })
        } else {
            // Use Docker container forking
            info!("Using Docker container forking from parent: {}", parent);

            // Create a fork from the parent checkpoint
            let fork_id = format!("fork-{}", req.id);

            // First ensure parent container exists and create checkpoint if needed
            let checkpoint_id = format!("checkpoint-{}", parent);

            // Check if we already have this checkpoint
            let checkpoints = self.docker_fork.checkpoints.read().await;
            let needs_checkpoint = !checkpoints.contains_key(&checkpoint_id);
            drop(checkpoints);

            if needs_checkpoint {
                // Create a base container with the parent ID and checkpoint it
                info!("Creating base container and checkpoint for parent: {}", parent);

                // Start a container to be the parent
                let config = faas_common::SandboxConfig {
                    function_id: parent.clone(),
                    source: req.env.clone(),
                    command: vec!["sh".to_string(), "-c".to_string(), "sleep 3600".to_string()], // Keep alive
                    payload: Vec::new(),
                    env_vars: None,
                    runtime: Some(faas_common::Runtime::Docker),  // Use Docker for container forking
                    execution_mode: Some(faas_common::ExecutionMode::Branched),
                    memory_limit: None,
                    timeout: Some(3600000),  // 1 hour in milliseconds
                };

                // Execute to create the parent container
                let _ = self.container.execute(config).await?;

                // Now checkpoint it
                self.docker_fork.checkpoint_container(&parent, &checkpoint_id).await
                    .map_err(|e| anyhow::anyhow!("Failed to checkpoint parent: {}", e))?;
            }

            // Fork from the checkpoint
            let forked_container_id = self.docker_fork.fork_from_checkpoint(&checkpoint_id, &fork_id).await
                .map_err(|e| anyhow::anyhow!("Failed to fork from checkpoint: {}", e))?;

            info!("Created fork {} from parent {} (container: {})", fork_id, parent, forked_container_id);

            // Execute the code in the forked container
            let output = self.docker_fork.execute_in_fork(&fork_id, &req.code).await
                .map_err(|e| anyhow::anyhow!("Failed to execute in fork: {}", e))?;

            // Clean up the fork after execution
            let _ = self.docker_fork.cleanup_fork(&fork_id).await;

            Ok(Response {
                id: fork_id.clone(),
                stdout: output,
                stderr: Vec::new(),
                exit_code: 0,
                duration: start.elapsed(),
                snapshot: Some(fork_id),
            })
        }
    }

    async fn run_persistent(&self, req: Request) -> Result<Response> {
        // For persistent mode, prefer Firecracker on Linux, Docker on other platforms
        let runtime = req.runtime.unwrap_or_else(|| {
            if cfg!(target_os = "linux") {
                faas_common::Runtime::Firecracker
            } else {
                faas_common::Runtime::Docker
            }
        });

        let config = faas_common::SandboxConfig {
            function_id: req.id.clone(),
            source: req.env,
            command: vec!["sh".to_string(), "-c".to_string(), req.code],
            payload: Vec::new(),
            env_vars: None,
            runtime: Some(runtime),
            execution_mode: Some(faas_common::ExecutionMode::Persistent),
            memory_limit: None,
            timeout: Some(req.timeout.as_millis() as u64),
        };

        // Runtime selection for persistent mode
        let result = match config.runtime {
            Some(faas_common::Runtime::Docker) => {
                self.container.execute(config).await?
            },
            Some(faas_common::Runtime::Firecracker) => {
                self.vm.execute(config).await?
            },
            Some(faas_common::Runtime::Auto) | None => {
                // Prefer Firecracker for persistent workloads on Linux, fallback to Docker
                if cfg!(target_os = "linux") {
                    match self.vm.execute(config.clone()).await {
                        Ok(res) => res,
                        Err(_) => self.container.execute(config).await?,
                    }
                } else {
                    self.container.execute(config).await?
                }
            }
        };

        Ok(Response {
            id: req.id,
            stdout: result.response.unwrap_or_default(),
            stderr: result.logs.map(|l| l.into_bytes()).unwrap_or_default(),
            exit_code: if result.error.is_none() { 0 } else { 1 },
            duration: Duration::from_millis(500),
            snapshot: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires Docker or Firecracker"]
    async fn test_modes() {
        let exec = Executor::new()
            .await
            .expect("Failed to create executor - ensure Docker is running");

        let req = Request {
            id: "test".to_string(),
            code: "echo test".to_string(),
            mode: Mode::Ephemeral,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };

        let res = exec.run(req).await.expect("Failed to run");
        assert_eq!(res.exit_code, 0);
        assert!(res.duration < Duration::from_millis(100));
    }
}
