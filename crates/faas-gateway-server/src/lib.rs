pub mod types;

use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicU64;

// Main request/response types
#[derive(Debug, Serialize, Deserialize)]
pub struct InvokeResponse {
    pub request_id: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub output: Option<String>,
    pub logs: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateInstanceRequest {
    pub name: Option<String>,
    pub image: String,
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSnapshotRequest {
    pub container_id: String,
    pub name: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrewarmRequest {
    pub image: String,
    pub count: usize,
    pub runtime: Option<faas_common::Runtime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub name: Option<String>,
    pub container_id: String,
    pub created_at: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: Option<String>,
    pub image: String,
    pub status: String,
    pub created_at: String,
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u32>,
}

// Metrics tracking
#[derive(Default)]
pub struct ExecutionMetrics {
    pub total_requests: AtomicU64,
    pub cache_hits: AtomicU64,
    pub docker_executions: AtomicU64,
    pub vm_executions: AtomicU64,
}

#[cfg(test)]
mod tests;