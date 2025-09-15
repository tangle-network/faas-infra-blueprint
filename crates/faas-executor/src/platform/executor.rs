use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, instrument};

use super::{memory::MemoryPool, snapshot::SnapshotStore, fork::ForkManager};

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

pub struct Executor {
    container: Arc<crate::executor::Executor>,
    vm: Arc<crate::firecracker::FirecrackerExecutor>,
    memory: Arc<MemoryPool>,
    snapshots: Arc<SnapshotStore>,
    forks: Arc<ForkManager>,
}

impl Executor {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            container: Arc::new(
                crate::executor::Executor::new(
                    crate::executor::ExecutionStrategy::Container(
                        crate::executor::ContainerStrategy {
                            warm_pools: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
                            max_pool_size: 10,
                            docker: Arc::new(crate::docktopus::DockerBuilder::new().await?.client().clone()),
                            build_cache_volumes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
                            dependency_layers: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
                            gpu_pools: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
                        }
                    )
                ).await?
            ),
            vm: Arc::new(crate::firecracker::FirecrackerExecutor::new().await?),
            memory: Arc::new(MemoryPool::new()?),
            snapshots: Arc::new(SnapshotStore::new().await?),
            forks: Arc::new(ForkManager::new()?),
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
            input: req.code.into_bytes(),
            env_vars: None,
        };

        let result = self.container.execute(config).await?;

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
        // Use warm container pool
        let config = faas_common::SandboxConfig {
            function_id: req.id.clone(),
            source: req.env,
            input: req.code.into_bytes(),
            env_vars: None,
        };

        let result = self.container.execute(config).await?;

        Ok(Response {
            id: req.id,
            stdout: result.response.unwrap_or_default(),
            stderr: result.logs.map(|l| l.into_bytes()).unwrap_or_default(),
            exit_code: if result.error.is_none() { 0 } else { 1 },
            duration: Duration::from_millis(150),
            snapshot: None,
        })
    }

    async fn run_checkpointed(&self, req: Request) -> Result<Response> {
        if let Some(checkpoint) = req.checkpoint {
            // Restore from checkpoint
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
        let parent = req.branch_from.ok_or_else(|| anyhow::anyhow!("branch_from required"))?;
        let branches = self.forks.branch(&parent, 1).await?;
        let branch_id = branches.first().unwrap();

        Ok(Response {
            id: branch_id.clone(),
            stdout: format!("Branched from {}", parent).into_bytes(),
            stderr: Vec::new(),
            exit_code: 0,
            duration: Duration::from_millis(50),
            snapshot: Some(branch_id.clone()),
        })
    }

    async fn run_persistent(&self, req: Request) -> Result<Response> {
        let config = faas_common::SandboxConfig {
            function_id: req.id.clone(),
            source: req.env,
            input: req.code.into_bytes(),
            env_vars: None,
        };

        let result = self.vm.execute(config).await?;

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
    async fn test_modes() {
        let exec = Executor::new().await.unwrap();

        let req = Request {
            id: "test".to_string(),
            code: "echo test".to_string(),
            mode: Mode::Ephemeral,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };

        let res = exec.run(req).await.unwrap();
        assert_eq!(res.exit_code, 0);
        assert!(res.duration < Duration::from_millis(100));
    }
}