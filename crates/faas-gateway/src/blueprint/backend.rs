//! Backend trait abstraction for Blueprint SDK integration
//!
//! Enables routing execution to either local containers or Tangle blockchain

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BackendError {
    #[error("Function not found: {0}")]
    NotFound(String),
    #[error("Function already exists: {0}")]
    AlreadyExists(String),
    #[error("Deployment failed: {0}")]
    DeploymentFailed(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Timeout after {0}s")]
    Timeout(u64),
}

pub type Result<T> = std::result::Result<T, BackendError>;

/// Configuration for function deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaasConfig {
    pub memory_mb: u32,
    pub timeout_secs: u64,
    #[serde(default)]
    pub max_concurrency: u32,
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
}

impl Default for FaasConfig {
    fn default() -> Self {
        Self {
            memory_mb: 512,
            timeout_secs: 300,
            max_concurrency: 10,
            env_vars: HashMap::new(),
        }
    }
}

/// Deployment information
#[derive(Debug, Serialize)]
pub struct DeployInfo {
    pub function_id: String,
    pub endpoint: String,
    pub status: String,
    pub cold_start_ms: u64,
    pub memory_mb: u32,
    pub timeout_secs: u64,
}

/// Invocation result
#[derive(Debug, Serialize)]
pub struct InvokeResult {
    pub job_id: u64,
    pub result: Vec<u8>,
    pub success: bool,
    pub execution_ms: u64,
}

/// Health status
#[derive(Debug, Serialize)]
pub struct HealthStatus {
    pub function_id: String,
    pub status: String,
    pub last_invocation: Option<String>,
    pub total_invocations: u64,
}

/// Execution backend trait
#[async_trait]
pub trait ExecutionBackend: Send + Sync {
    /// Deploy a function binary
    async fn deploy(
        &self,
        function_id: String,
        binary: Vec<u8>,
        config: FaasConfig,
    ) -> Result<DeployInfo>;

    /// Invoke a deployed function
    async fn invoke(&self, function_id: String, payload: Vec<u8>) -> Result<InvokeResult>;

    /// Check function health
    async fn health(&self, function_id: String) -> Result<HealthStatus>;

    /// Get function information
    async fn info(&self, function_id: String) -> Result<DeployInfo>;

    /// Undeploy a function
    async fn undeploy(&self, function_id: String) -> Result<()>;

    /// Pre-warm function containers (optional, may not apply to all backends)
    async fn warm(&self, function_id: String) -> Result<u32> {
        // Default: no-op
        Ok(0)
    }
}

/// Backend selection strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    /// Local execution (fast, no blockchain)
    Local,
    /// Tangle blockchain execution (decentralized, crypto payments)
    Tangle,
}

impl Default for BackendType {
    fn default() -> Self {
        Self::Local
    }
}

impl std::str::FromStr for BackendType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "tangle" => Ok(Self::Tangle),
            _ => Err(format!("Invalid backend type: {}", s)),
        }
    }
}
