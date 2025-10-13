// API types for FaaS gateway

use faas_common::InvocationResult;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Blueprint SDK integration
pub mod blueprint;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Internal server error: {0}")]
    Internal(String),
    #[error("Service unavailable")]
    ServiceUnavailable,
}

#[derive(Debug, Deserialize)]
pub struct InvokeRequest {
    pub image: String,
    pub command: Vec<String>,
    pub env_vars: Option<Vec<String>>,
    pub payload: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InvokeResponse {
    pub request_id: String,
    pub output: Option<String>,
    pub logs: Option<String>,
    pub error: Option<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

impl From<InvocationResult> for InvokeResponse {
    fn from(result: InvocationResult) -> Self {
        Self {
            request_id: result.request_id,
            output: result.response.and_then(|b| String::from_utf8(b).ok()),
            logs: result.logs,
            error: result.error,
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 0,
        }
    }
}

// Request/Response types for SDK compatibility
#[derive(Debug, Deserialize, Serialize)]
pub struct ExecuteRequest {
    pub command: String,
    pub image: Option<String>,
    pub env_vars: Option<Vec<(String, String)>>,
    pub working_dir: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateSnapshotRequest {
    pub name: String,
    pub container_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateInstanceRequest {
    pub name: Option<String>,
    pub image: String,
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u32>,
}