use faas_common::{InvocationResult, SandboxConfig, SandboxExecutor};
use faas_executor::DockerExecutor;
use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;

/// Configuration for executing functions
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub image: String,
    pub command: String,
    pub env_vars: Vec<(String, String)>,
    pub working_dir: Option<String>,
    pub timeout: Duration,
    pub memory_limit: Option<u64>,
    pub cpu_limit: Option<f64>,
}

/// Result of function execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

/// Wrapper for DockerExecutor with custom execute method
pub struct ExecutorWrapper {
    inner: Arc<DockerExecutor>,
}

impl ExecutorWrapper {
    pub fn new(executor: Arc<DockerExecutor>) -> Self {
        Self { inner: executor }
    }

    /// Execute a function with the given configuration
    pub async fn execute(&self, config: ExecutionConfig) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();

        // Convert to SandboxConfig for the trait implementation
        let sandbox_config = SandboxConfig {
            function_id: uuid::Uuid::new_v4().to_string(),
            source: config.image.clone(),
            command: vec!["/bin/sh".to_string(), "-c".to_string(), config.command],
            env_vars: Some(config.env_vars.into_iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect()),
            payload: vec![],
        };

        // Use the SandboxExecutor trait method
        let result: InvocationResult = self.inner.execute(sandbox_config).await?;

        Ok(ExecutionResult {
            exit_code: if result.error.is_none() { 0 } else { 1 },
            stdout: result.response
                .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
                .unwrap_or_default(),
            stderr: result.error.unwrap_or_default(),
            duration: start.elapsed(),
        })
    }
}