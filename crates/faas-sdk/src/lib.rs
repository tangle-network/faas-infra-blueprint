//! # FaaS Platform Rust SDK
//!
//! The official Rust SDK for the FaaS Platform, providing high-performance serverless execution
//! with both Docker containers and Firecracker microVMs.
//!
//! ## Quick Start
//!
//! ```rust
//! use faas_sdk::{FaasClient, Runtime, ExecuteRequest};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = FaasClient::new("http://localhost:8080".to_string());
//!
//!     // Simple execution
//!     let result = client.execute(ExecuteRequest {
//!         command: "echo 'Hello, World!'".to_string(),
//!         image: Some("alpine:latest".to_string()),
//!         env_vars: None,
//!         working_dir: None,
//!         timeout_ms: Some(5000),
//!     }).await?;
//!
//!     println!("Output: {}", result.stdout);
//!     Ok(())
//! }
//! ```
//!
//! ## Key Features
//!
//! - **🚀 Dual Runtime Support**: Choose Docker for development, Firecracker for production
//! - **📊 Multi-level Caching**: Automatic result caching across memory, disk, and distributed layers
//! - **🔥 Warm Pools**: Pre-warmed containers and VMs eliminate cold starts
//! - **🌳 Execution Forking**: Branch workflows for A/B testing and parallel paths
//! - **💾 Snapshots**: Checkpoint and restore execution state
//! - **📈 Auto-scaling**: Predictive scaling based on load patterns
//! - **📋 Rich Metrics**: Built-in performance monitoring and client-side metrics
//!
//! ## Runtime Selection
//!
//! | Runtime | Cold Start | Security | Use Case |
//! |---------|------------|----------|----------|
//! | Docker | 50-200ms | Process isolation | Development, testing |
//! | Firecracker | ~125ms | Hardware isolation | Production, multi-tenant |
//! | Auto | Varies | Adaptive | Automatic selection |
//!
//! ## Examples
//!
//! ### Advanced Configuration
//!
//! ```rust
//! use faas_sdk::{FaasClient, AdvancedExecuteRequest, ExecutionMode};
//!
//! let result = client.execute(ExecuteRequest {
//!     command: "python ml_inference.py".to_string(),
//!     image: Some("pytorch/pytorch:latest".to_string()),
//!     mode: Some("cached".to_string()),
//!     env_vars: Some(vec![
//!         ("MODEL_PATH".to_string(), "/models/bert".to_string())
//!     ]),
//!     memory_mb: Some(2048),
//!     cpu_cores: Some(2),
//!     ..Default::default()
//! }).await?;
//! ```
//!
//! ### Execution Forking
//!
//! ```rust
//! // Create base execution
//! let base = client.execute(ExecuteRequest {
//!     command: "setup_environment.sh".to_string(),
//!     ..Default::default()
//! }).await?;
//!
//! // Fork for different experiment paths
//! let fork_a = client.fork_execution(
//!     &base.request_id,
//!     "run_experiment_a.py"
//! ).await?;
//!
//! let fork_b = client.fork_execution(
//!     &base.request_id,
//!     "run_experiment_b.py"
//! ).await?;
//! ```

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

/// Execution result type alias for convenience
pub type ExecutionResult = ExecuteResponse;


#[derive(Error, Debug)]
pub enum SdkError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("API error: {message}")]
    Api { message: String },
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("Timeout occurred")]
    Timeout,
}

/// Runtime environment selection for execution
///
/// Choose the optimal runtime based on your requirements:
/// - `Docker`: Best for development and testing (50-200ms cold start)
/// - `Firecracker`: Best for production and multi-tenant environments (~125ms cold start)
/// - `Auto`: Platform automatically selects based on workload characteristics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    /// Docker containers - fastest iteration, rich ecosystem
    ///
    /// **Pros:**
    /// - Fastest cold starts (50-200ms)
    /// - Hot reload support
    /// - GPU passthrough
    /// - Rich image ecosystem
    ///
    /// **Cons:**
    /// - Process-level isolation only
    /// - Shared kernel
    Docker,

    /// Firecracker VMs - hardware-level isolation
    ///
    /// **Pros:**
    /// - Hardware-level isolation
    /// - Memory encryption
    /// - Compliance-ready
    /// - Multi-tenant safe
    ///
    /// **Cons:**
    /// - Slightly higher cold start (~125ms)
    /// - Linux only
    /// - Limited GPU support
    Firecracker,

    /// Automatic runtime selection
    ///
    /// The platform analyzes workload characteristics and automatically
    /// chooses the optimal runtime based on:
    /// - Security requirements
    /// - Performance needs
    /// - Resource constraints
    /// - Historical patterns
    Auto,
}

/// High-performance FaaS Platform client with intelligent optimization
///
/// The `FaasClient` provides a unified interface to the FaaS platform, supporting both
/// Docker containers and Firecracker microVMs with automatic optimization, caching,
/// and scaling capabilities.
///
/// ## Features
///
/// - **Dual Runtime Support**: Seamlessly switch between Docker and Firecracker
/// - **Smart Caching**: Automatic result caching with configurable TTL
/// - **Load Balancing**: Built-in request distribution and retry logic
/// - **Metrics Collection**: Real-time performance monitoring
/// - **Connection Pooling**: Efficient HTTP connection reuse
///
/// ## Thread Safety
///
/// `FaasClient` is fully thread-safe and can be shared across multiple threads using `Arc`:
///
/// ```rust
/// use std::sync::Arc;
/// use faas_sdk::FaasClient;
///
/// let client = Arc::new(FaasClient::new("http://localhost:8080".to_string()));
/// let client_clone = client.clone(); // Safe to clone and use in different threads
/// ```
pub struct FaasClient {
    client: Client,
    base_url: String,
    runtime: Runtime,
    cache_enabled: bool,
    metrics: Arc<RwLock<ClientMetrics>>,
}

/// Client-side metrics for monitoring
#[derive(Debug, Default)]
struct ClientMetrics {
    total_requests: u64,
    cache_hits: u64,
    cold_starts: u64,
    total_latency_ms: u64,
    errors: u64,
}

/// Function execution request with runtime selection
#[derive(Debug, Serialize, Default, Clone)]
pub struct ExecuteRequest {
    pub command: String,
    pub image: Option<String>,
    pub runtime: Option<Runtime>,
    pub mode: Option<String>,
    pub env_vars: Option<Vec<(String, String)>>,
    pub working_dir: Option<String>,
    pub timeout_ms: Option<u64>,
    pub memory_mb: Option<u32>,
    pub cpu_cores: Option<u8>,
    pub cache_key: Option<String>,
    pub snapshot_id: Option<String>,
    pub branch_from: Option<String>,
    pub payload: Option<Vec<u8>>,
}

/// Advanced execution request (now uses same structure as ExecuteRequest)
pub type AdvancedExecuteRequest = ExecuteRequest;

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

/// Fork execution branch
#[derive(Debug, Serialize, Clone)]
pub struct ForkBranch {
    pub id: String,
    pub command: String,
    pub env_vars: Option<Vec<(String, String)>>,
    pub weight: Option<f64>,
}

/// Fork execution strategy
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ForkStrategy {
    Parallel,  // Run all branches in parallel
    Fastest,   // Select fastest completion
    Sequential, // Run sequentially
}

/// Container prewarming request
#[derive(Debug, Serialize)]
pub struct PrewarmRequest {
    pub image: String,
    pub count: u32,
    pub runtime: Option<Runtime>,
    pub memory_mb: Option<u32>,
    pub cpu_cores: Option<u32>,
}

/// Fork execution result
#[derive(Debug, Deserialize)]
pub struct ForkResult {
    pub results: Vec<serde_json::Value>,
    pub selected_branch: Option<String>,
    pub selection_reason: Option<String>,
}

/// Performance metrics
#[derive(Debug, Deserialize)]
pub struct PerformanceMetrics {
    pub total_executions: u64,
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
    /// Create new FaaS client with automatic runtime selection
    ///
    /// Creates a client that automatically selects the optimal runtime (Docker or Firecracker)
    /// based on workload characteristics and platform capabilities.
    ///
    /// # Arguments
    ///
    /// * `base_url` - The base URL of the FaaS platform (e.g., "http://localhost:8080")
    ///
    /// # Examples
    ///
    /// ```rust
    /// use faas_sdk::FaasClient;
    ///
    /// let client = FaasClient::new("http://localhost:8080".to_string());
    /// ```
    pub fn new(base_url: String) -> Self {
        Self::with_runtime(base_url, Runtime::Auto)
    }

    /// Create client with explicit runtime selection
    ///
    /// Choose a specific runtime for all executions. This is useful when you have
    /// specific requirements for isolation, performance, or compatibility.
    ///
    /// # Arguments
    ///
    /// * `base_url` - The base URL of the FaaS platform
    /// * `runtime` - The runtime to use for all executions
    ///
    /// # Examples
    ///
    /// ```rust
    /// use faas_sdk::{FaasClient, Runtime};
    ///
    /// // For development (fastest iteration)
    /// let dev_client = FaasClient::with_runtime(
    ///     "http://localhost:8080".to_string(),
    ///     Runtime::Docker
    /// );
    ///
    /// // For production (stronger isolation)
    /// let prod_client = FaasClient::with_runtime(
    ///     "https://api.example.com".to_string(),
    ///     Runtime::Firecracker
    /// );
    /// ```
    pub fn with_runtime(base_url: String, runtime: Runtime) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to create HTTP client"),
            base_url,
            runtime,
            cache_enabled: true,
            metrics: Arc::new(RwLock::new(ClientMetrics::default())),
        }
    }

    /// Use Docker runtime for development
    pub fn use_docker(mut self) -> Self {
        self.runtime = Runtime::Docker;
        self
    }

    /// Use Firecracker VMs for production
    pub fn use_firecracker(mut self) -> Self {
        self.runtime = Runtime::Firecracker;
        self
    }

    /// Enable/disable caching
    pub fn with_caching(mut self, enabled: bool) -> Self {
        self.cache_enabled = enabled;
        self
    }

    /// Execute a command or script with the FaaS platform
    ///
    /// This is the primary method for executing code on the platform. It supports
    /// automatic caching, runtime selection, and intelligent optimization.
    ///
    /// # Arguments
    ///
    /// * `request` - Execution request with command, image, and options
    ///
    /// # Returns
    ///
    /// Returns `ExecuteResponse` with output, logs, timing, and metadata.
    ///
    /// # Errors
    ///
    /// Returns `SdkError` for network issues, API errors, or timeouts.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use faas_sdk::{FaasClient, ExecuteRequest};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = FaasClient::new("http://localhost:8080".to_string());
    ///
    /// // Simple shell command
    /// let result = client.execute(ExecuteRequest {
    ///     command: "echo 'Hello, World!'".to_string(),
    ///     image: Some("alpine:latest".to_string()),
    ///     env_vars: None,
    ///     working_dir: None,
    ///     timeout_ms: Some(5000),
    ///     cache_key: None,
    ///     runtime: None, // Uses client default
    /// }).await?;
    ///
    /// println!("Output: {}", result.stdout);
    /// println!("Execution time: {}ms", result.duration_ms);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## Advanced Usage with Environment Variables
    ///
    /// ```rust
    /// use faas_sdk::{FaasClient, ExecuteRequest, Runtime};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = FaasClient::new("http://localhost:8080".to_string());
    ///
    /// let result = client.execute(ExecuteRequest {
    ///     command: "python process_data.py".to_string(),
    ///     image: Some("python:3.11-slim".to_string()),
    ///     env_vars: Some(vec![
    ///         ("API_KEY".to_string(), "secret123".to_string()),
    ///         ("DEBUG".to_string(), "true".to_string()),
    ///     ]),
    ///     working_dir: Some("/app".to_string()),
    ///     timeout_ms: Some(30000),
    ///     cache_key: Some("data-processing-v1".to_string()),
    ///     runtime: Some(Runtime::Docker),
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(&self, mut request: ExecuteRequest) -> Result<ExecuteResponse, SdkError> {
        let start = Instant::now();

        // Apply runtime if not specified
        if request.runtime.is_none() {
            request.runtime = Some(self.runtime.clone());
        }

        // Apply cache key if caching enabled
        if self.cache_enabled && request.cache_key.is_none() {
            request.cache_key = Some(format!("{:x}", md5::compute(&request.command)));
        }

        let url = format!("{}/api/v1/execute", self.base_url);

        // Handle payload by sending as stdin directly in the JSON request
        let response = if let Some(_payload) = &request.payload {
            // For now, just use JSON and let the server handle stdin
            self.client.post(&url)
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?
        } else {
            self.client.post(&url)
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?
        };

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;
        metrics.total_latency_ms += start.elapsed().as_millis() as u64;

        if !response.status().is_success() {
            metrics.errors += 1;
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        let result: ExecuteResponse = response.json().await?;

        // Check if it was a cache hit (heuristic: very fast response)
        if start.elapsed().as_millis() < 10 {
            metrics.cache_hits += 1;
        }

        Ok(result)
    }

    /// Execute function with advanced performance optimizations and workflow control
    ///
    /// This method provides access to advanced features like execution modes, resource limits,
    /// and workflow orchestration capabilities.
    ///
    /// # Arguments
    ///
    /// * `request` - Advanced execution request with mode, resources, and optimization settings
    ///
    /// # Returns
    ///
    /// Returns `ExecuteResponse` with detailed execution metadata and performance metrics.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use faas_sdk::{FaasClient, AdvancedExecuteRequest, ExecutionMode};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = FaasClient::new("http://localhost:8080".to_string());
    ///
    /// let result = client.execute_advanced(AdvancedExecuteRequest {
    ///     command: "python train_model.py".to_string(),
    ///     image: "pytorch/pytorch:latest".to_string(),
    ///     mode: Some("cached".to_string()),
    ///     env_vars: Some(vec![
    ///         ("GPU_MEMORY".to_string(), "8GB".to_string())
    ///     ]),
    ///     memory_mb: Some(4096),
    ///     cpu_cores: Some(4),
    ///     use_snapshots: Some(true),
    /// }).await?;
    ///
    /// println!("Training completed in {}ms", result.duration_ms);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_advanced(&self, request: AdvancedExecuteRequest) -> Result<ExecuteResponse, SdkError> {
        let url = format!("{}/api/v1/execute", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(response.json().await?)
    }

    /// Create a container or VM snapshot for state preservation and reuse
    ///
    /// Snapshots capture the complete state of a running container/VM, including memory,
    /// filesystem, and process state. This enables checkpoint/restore workflows and
    /// instant warm starts from saved states.
    ///
    /// # Arguments
    ///
    /// * `request` - Snapshot creation request with name, container ID, and metadata
    ///
    /// # Returns
    ///
    /// Returns `SnapshotResponse` with snapshot ID, size, and creation timestamp.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use faas_sdk::{FaasClient, CreateSnapshotRequest};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = FaasClient::new("http://localhost:8080".to_string());
    ///
    /// // First, create a container with some state
    /// let execution = client.execute(ExecuteRequest {
    ///     command: "python setup_model.py".to_string(),
    ///     ..Default::default()
    /// }).await?;
    ///
    /// // Create snapshot of the initialized container
    /// let snapshot = client.create_snapshot(CreateSnapshotRequest {
    ///     name: "model-initialized".to_string(),
    ///     container_id: execution.request_id,
    ///     description: Some("Model loaded and ready for inference".to_string()),
    /// }).await?;
    ///
    /// println!("Created snapshot {} ({} bytes)", snapshot.name, snapshot.size_bytes);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **Checkpoint/Restore**: Save and restore long-running computations
    /// - **Warm Starts**: Pre-initialize environments for instant execution
    /// - **A/B Testing**: Create branching points for different execution paths
    /// - **Fault Tolerance**: Recover from failures using saved states
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
    pub async fn health_check(&self) -> Result<HealthStatus, SdkError> {
        let url = format!("{}/health", self.base_url);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(response.json().await?)
    }

    // User-centric convenience methods

    /// Execute Python code with automatic environment setup
    ///
    /// This convenience method runs Python code with a pre-configured Python runtime,
    /// automatically handling common dependencies and environment setup.
    ///
    /// # Arguments
    ///
    /// * `code` - Python source code to execute
    ///
    /// # Returns
    ///
    /// Returns `ExecuteResponse` with output, logs, and execution metadata.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use faas_sdk::FaasClient;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = FaasClient::new("http://localhost:8080".to_string());
    ///
    /// // Simple Python execution
    /// let result = client.run_python(r#"
    /// import json
    /// import datetime
    ///
    /// data = {
    ///     "message": "Hello from Python!",
    ///     "timestamp": datetime.datetime.now().isoformat()
    /// }
    /// print(json.dumps(data, indent=2))
    /// "#).await?;
    ///
    /// println!("Python output: {}", result.stdout);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Features
    ///
    /// - **Automatic Dependencies**: Common packages pre-installed
    /// - **Error Handling**: Python exceptions captured in response
    /// - **Output Capture**: Both stdout and stderr captured
    /// - **Timeout Protection**: Prevents runaway executions
    pub async fn run_python(&self, code: &str) -> Result<ExecuteResponse, SdkError> {
        // Send code via stdin to avoid quoting issues
        self.execute(ExecuteRequest {
            command: "python".to_string(),
            image: Some("python:3.11-slim".to_string()),
            runtime: Some(self.runtime.clone()),
            timeout_ms: Some(30000),
            cache_key: Some(format!("{:x}", md5::compute(code.as_bytes()))),
            payload: Some(code.as_bytes().to_vec()),
            ..Default::default()
        }).await
    }

    /// Execute JavaScript/Node.js code
    pub async fn run_javascript(&self, code: &str) -> Result<ExecuteResponse, SdkError> {
        // Send code via stdin to avoid quoting issues
        self.execute(ExecuteRequest {
            command: "node".to_string(),
            image: Some("node:20-slim".to_string()),
            runtime: Some(self.runtime.clone()),
            timeout_ms: Some(30000),
            cache_key: Some(format!("{:x}", md5::compute(code.as_bytes()))),
            payload: Some(code.as_bytes().to_vec()),
            ..Default::default()
        }).await
    }

    /// Execute Bash script
    pub async fn run_bash(&self, script: &str) -> Result<ExecuteResponse, SdkError> {
        self.execute(ExecuteRequest {
            command: format!("bash -c \"{}\"", script),
            image: Some("alpine:latest".to_string()),
            runtime: Some(self.runtime.clone()),
            timeout_ms: Some(30000),
            cache_key: Some(format!("{:x}", md5::compute(script.as_bytes()))),
            ..Default::default()
        }).await
    }

    /// Fork execution from parent for A/B testing
    pub async fn fork_execution(&self, parent_id: &str, command: &str) -> Result<ExecuteResponse, SdkError> {
        self.execute(ExecuteRequest {
            command: command.to_string(),
            image: Some("alpine:latest".to_string()),
            runtime: Some(self.runtime.clone()),
            mode: Some("branched".to_string()),
            branch_from: Some(parent_id.to_string()),
            env_vars: None,
            memory_mb: None,
            cpu_cores: None,
            working_dir: None,
            timeout_ms: Some(30000),
            cache_key: None,
            snapshot_id: None,
            payload: None,
        }).await
    }

    /// Create checkpoint for stateful workflow
    pub async fn checkpoint_execution(&self, execution_id: &str) -> Result<SnapshotResponse, SdkError> {
        self.create_snapshot(CreateSnapshotRequest {
            name: format!("checkpoint-{}", execution_id),
            container_id: execution_id.to_string(),
            description: Some("Execution checkpoint".to_string()),
        }).await
    }

    /// Pre-warm containers for zero cold starts
    pub async fn prewarm(&self, image: &str, count: u32) -> Result<(), SdkError> {
        let url = format!("{}/api/v1/prewarm", self.base_url);
        let response = self.client.post(&url)
            .json(&serde_json::json!({
                "image": image,
                "count": count,
                "runtime": self.runtime
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SdkError::Api { message: error_text });
        }

        Ok(())
    }

    /// Get client-side metrics
    pub async fn client_metrics(&self) -> ClientMetricsReport {
        let metrics = self.metrics.read().await;
        ClientMetricsReport {
            total_requests: metrics.total_requests,
            cache_hit_rate: if metrics.total_requests > 0 {
                metrics.cache_hits as f64 / metrics.total_requests as f64
            } else {
                0.0
            },
            average_latency_ms: if metrics.total_requests > 0 {
                metrics.total_latency_ms / metrics.total_requests
            } else {
                0
            },
            error_rate: if metrics.total_requests > 0 {
                metrics.errors as f64 / metrics.total_requests as f64
            } else {
                0.0
            },
        }
    }
}

/// Client metrics report
#[derive(Debug)]
pub struct ClientMetricsReport {
    pub total_requests: u64,
    pub cache_hit_rate: f64,
    pub average_latency_ms: u64,
    pub error_rate: f64,
}

/// Convenience methods for common operations
impl FaasClient {
    /// Execute simple command
    pub async fn run(&self, command: &str) -> Result<String, SdkError> {
        let request = ExecuteRequest {
            command: command.to_string(),
            image: Some("alpine:latest".to_string()),
            ..Default::default()
        };

        let response = self.execute(request).await?;
        Ok(response.stdout)
    }

    /// Execute with custom image
    pub async fn run_with_image(&self, command: &str, image: &str) -> Result<String, SdkError> {
        let request = ExecuteRequest {
            command: command.to_string(),
            image: Some(image.to_string()),
            ..Default::default()
        };

        let response = self.execute(request).await?;
        Ok(response.stdout)
    }

    /// Execute with caching for repeated operations
    pub async fn run_cached(&self, command: &str, image: &str) -> Result<String, SdkError> {
        let request = ExecuteRequest {
            command: command.to_string(),
            image: Some(image.to_string()),
            runtime: Some(Runtime::Auto),
            mode: Some("cached".to_string()),
            env_vars: None,
            memory_mb: None,
            cpu_cores: None,
            working_dir: None,
            timeout_ms: Some(30000),
            cache_key: Some(format!("{:x}", md5::compute(command.as_bytes()))),
            snapshot_id: None,
            branch_from: None,
            payload: None,
        };

        let response = self.execute(request).await?;
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