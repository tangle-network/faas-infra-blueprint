//! FaaS Platform SDK
//!
//! Comprehensive client SDK for all platform features including:
//! - Function execution with container pooling
//! - Snapshot management and caching
//! - Instance creation and lifecycle
//! - Performance monitoring and metrics

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SdkError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("API error: {message}")]
    Api { message: String },
    #[error("Timeout occurred")]
    Timeout,
}

/// FaaS Platform Client
pub struct FaasClient {
    client: Client,
    base_url: String,
}

/// Function execution request
#[derive(Debug, Serialize)]
pub struct ExecuteRequest {
    pub command: String,
    pub image: Option<String>,
    pub env_vars: Option<Vec<(String, String)>>,
    pub working_dir: Option<String>,
    pub timeout_ms: Option<u64>,
}

/// Advanced execution with performance optimizations
#[derive(Debug, Serialize)]
pub struct AdvancedExecuteRequest {
    pub command: String,
    pub image: String,
    pub mode: ExecutionMode,
    pub env_vars: Option<Vec<(String, String)>>,
    pub memory_mb: Option<u32>,
    pub cpu_cores: Option<u32>,
    pub use_snapshots: Option<bool>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    Ephemeral,   // No persistence
    Cached,      // Use cache for repeated executions
    Checkpointed,// CRIU checkpoint/restore
    Branched,    // Fork from base environment
}

/// Function execution response
#[derive(Debug, Deserialize)]
pub struct ExecuteResponse {
    pub request_id: String,
    pub output: Option<String>,
    pub logs: Option<String>,
    pub error: Option<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// Snapshot management
#[derive(Debug, Serialize)]
pub struct CreateSnapshotRequest {
    pub name: String,
    pub container_id: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SnapshotResponse {
    pub snapshot_id: String,
    pub name: String,
    pub size_bytes: u64,
    pub created_at: String,
}

/// Instance management
#[derive(Debug, Serialize)]
pub struct CreateInstanceRequest {
    pub name: Option<String>,
    pub image: String,
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u32>,
    pub persistent: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct InstanceResponse {
    pub instance_id: String,
    pub status: String,
    pub created_at: String,
    pub endpoints: Option<HashMap<String, String>>,
}

/// Performance metrics
#[derive(Debug, Deserialize)]
pub struct PerformanceMetrics {
    pub avg_execution_time_ms: f64,
    pub cache_hit_rate: f64,
    pub active_containers: u32,
    pub active_instances: u32,
    pub memory_usage_mb: u64,
    pub cpu_usage_percent: f64,
}

/// Health status
#[derive(Debug, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: String,
    pub components: Option<HashMap<String, String>>,
}

impl FaasClient {
    /// Create new FaaS client
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to create HTTP client"),
            base_url,
        }
    }

    /// Execute function with basic options
    pub async fn execute(&self, request: ExecuteRequest) -> Result<ExecuteResponse, SdkError> {
        let url = format!("{}/api/v1/execute", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(response.json().await?)
    }

    /// Execute function with advanced performance optimizations
    pub async fn execute_advanced(&self, request: AdvancedExecuteRequest) -> Result<ExecuteResponse, SdkError> {
        let url = format!("{}/api/v1/execute/advanced", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(response.json().await?)
    }

    /// Create container snapshot for reuse
    pub async fn create_snapshot(&self, request: CreateSnapshotRequest) -> Result<SnapshotResponse, SdkError> {
        let url = format!("{}/api/v1/snapshots", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(response.json().await?)
    }

    /// List available snapshots
    pub async fn list_snapshots(&self) -> Result<Vec<SnapshotResponse>, SdkError> {
        let url = format!("{}/api/v1/snapshots", self.base_url);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(response.json().await?)
    }

    /// Delete snapshot
    pub async fn delete_snapshot(&self, snapshot_id: &str) -> Result<(), SdkError> {
        let url = format!("{}/api/v1/snapshots/{}", self.base_url, snapshot_id);
        let response = self.client.delete(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(())
    }

    /// Create persistent instance
    pub async fn create_instance(&self, request: CreateInstanceRequest) -> Result<InstanceResponse, SdkError> {
        let url = format!("{}/api/v1/instances", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(response.json().await?)
    }

    /// List active instances
    pub async fn list_instances(&self) -> Result<Vec<InstanceResponse>, SdkError> {
        let url = format!("{}/api/v1/instances", self.base_url);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(response.json().await?)
    }

    /// Stop instance
    pub async fn stop_instance(&self, instance_id: &str) -> Result<(), SdkError> {
        let url = format!("{}/api/v1/instances/{}/stop", self.base_url, instance_id);
        let response = self.client.post(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(())
    }

    /// Delete instance
    pub async fn delete_instance(&self, instance_id: &str) -> Result<(), SdkError> {
        let url = format!("{}/api/v1/instances/{}", self.base_url, instance_id);
        let response = self.client.delete(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(())
    }

    /// Get performance metrics
    pub async fn get_metrics(&self) -> Result<PerformanceMetrics, SdkError> {
        let url = format!("{}/api/v1/metrics", self.base_url);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(response.json().await?)
    }

    /// Check health status
    pub async fn health(&self) -> Result<HealthStatus, SdkError> {
        let url = format!("{}/health", self.base_url);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(response.json().await?)
    }
}

/// Convenience methods for common operations
impl FaasClient {
    /// Execute simple command
    pub async fn run(&self, command: &str) -> Result<String, SdkError> {
        let request = ExecuteRequest {
            command: command.to_string(),
            image: Some("alpine:latest".to_string()),
            env_vars: None,
            working_dir: None,
            timeout_ms: None,
        };

        let response = self.execute(request).await?;
        Ok(response.stdout)
    }

    /// Execute with custom image
    pub async fn run_with_image(&self, command: &str, image: &str) -> Result<String, SdkError> {
        let request = ExecuteRequest {
            command: command.to_string(),
            image: Some(image.to_string()),
            env_vars: None,
            working_dir: None,
            timeout_ms: None,
        };

        let response = self.execute(request).await?;
        Ok(response.stdout)
    }

    /// Execute with caching for repeated operations
    pub async fn run_cached(&self, command: &str, image: &str) -> Result<String, SdkError> {
        let request = AdvancedExecuteRequest {
            command: command.to_string(),
            image: image.to_string(),
            mode: ExecutionMode::Cached,
            env_vars: None,
            memory_mb: None,
            cpu_cores: None,
            use_snapshots: Some(true),
        };

        let response = self.execute_advanced(request).await?;
        Ok(response.stdout)
    }

    /// Create development environment with persistence
    pub async fn create_dev_env(&self, name: &str, image: &str) -> Result<String, SdkError> {
        let request = CreateInstanceRequest {
            name: Some(name.to_string()),
            image: image.to_string(),
            cpu_cores: Some(2),
            memory_mb: Some(2048),
            persistent: Some(true),
        };

        let response = self.create_instance(request).await?;
        Ok(response.instance_id)
    }
}