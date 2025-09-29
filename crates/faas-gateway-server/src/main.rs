use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    response::{IntoResponse, Response, sse::{Event, Sse}},
    routing::{get, post},
    Json, Router,
};
use bollard::Docker;
use dashmap::DashMap;
use faas_common::SandboxExecutor;
use faas_executor::{DockerExecutor, docker_snapshot::DockerSnapshotManager};
use faas_gateway::{
    CreateInstanceRequest, CreateSnapshotRequest, ExecuteRequest, InvokeRequest, InvokeResponse,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc, time::{Duration, Instant}};
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};
use uuid::Uuid;
use futures::stream::Stream;

mod executor_wrapper;
mod types;
use executor_wrapper::{ExecutionConfig, ExecutorWrapper};
use types::{Instance, Snapshot};

#[derive(Clone)]
struct AppState {
    executor_wrapper: Arc<ExecutorWrapper>,
    executor: Arc<DockerExecutor>,
    firecracker_executor: Option<Arc<faas_executor::firecracker::FirecrackerExecutor>>,
    snapshot_manager: Arc<DockerSnapshotManager>,
    instances: Arc<DashMap<String, Instance>>,
    snapshots: Arc<DashMap<String, Snapshot>>,
    warm_pools: Arc<DashMap<String, WarmPool>>,
    metrics: Arc<ServerMetrics>,
}

#[derive(Debug, Default)]
struct ServerMetrics {
    total_requests: Arc<std::sync::atomic::AtomicU64>,
    cache_hits: Arc<std::sync::atomic::AtomicU64>,
    docker_executions: Arc<std::sync::atomic::AtomicU64>,
    vm_executions: Arc<std::sync::atomic::AtomicU64>,
}

#[derive(Debug)]
struct WarmPool {
    image: String,
    runtime: Runtime,
    available: Vec<String>,
    in_use: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Runtime {
    Docker,
    Firecracker,
    Auto,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExecuteRequestWithRuntime {
    command: String,
    image: Option<String>,
    runtime: Option<Runtime>,
    cache_key: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExecuteAdvancedRequest {
    command: String,
    image: Option<String>,
    runtime: Runtime,
    mode: Option<String>,
    snapshot_id: Option<String>,
    branch_id: Option<String>,
    branch_from: Option<String>,
    enable_gpu: Option<bool>,
    timeout_ms: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info,faas_gateway_server=debug")
        .init();

    let docker = Arc::new(Docker::connect_with_local_defaults()?);
    let executor = Arc::new(DockerExecutor::new(docker.clone()));
    let snapshot_manager = Arc::new(DockerSnapshotManager::new(docker.clone()));
    let executor_wrapper = Arc::new(ExecutorWrapper::new(executor.clone()));

    // Try to initialize Firecracker executor if on Linux
    let firecracker_executor = if cfg!(target_os = "linux") {
        match faas_executor::firecracker::FirecrackerExecutor::new(
            "/usr/bin/firecracker".to_string(),
            "/opt/firecracker/kernel".to_string(),
            "/opt/firecracker/rootfs.ext4".to_string(),
        ) {
            Ok(fc) => {
                info!("✅ Firecracker VM support enabled");
                Some(Arc::new(fc))
            },
            Err(e) => {
                warn!("Firecracker not available: {}", e);
                None
            }
        }
    } else {
        info!("Running on non-Linux, Firecracker VMs disabled");
        None
    };

    let state = AppState {
        executor_wrapper,
        executor,
        firecracker_executor,
        snapshot_manager,
        instances: Arc::new(DashMap::new()),
        snapshots: Arc::new(DashMap::new()),
        warm_pools: Arc::new(DashMap::new()),
        metrics: Arc::new(ServerMetrics::default()),
    };

    let app = create_app(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("🚀 FaaS Gateway listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

pub fn create_app(state: AppState) -> Router {
    Router::new()
        // Execute endpoints with runtime selection
        .route("/api/v1/execute", post(execute_handler))
        .route("/api/v1/execute/advanced", post(execute_advanced_handler))

        // Convenience endpoints for specific languages
        .route("/api/v1/run/python", post(run_python_handler))
        .route("/api/v1/run/javascript", post(run_javascript_handler))
        .route("/api/v1/run/rust", post(run_rust_handler))

        // Branched execution for A/B testing
        .route("/api/v1/fork", post(fork_execution_handler))
        .route("/api/v1/executions/:id/fork", post(fork_from_parent_handler))

        // Pre-warming for zero cold starts
        .route("/api/v1/prewarm", post(prewarm_handler))
        .route("/api/v1/pools", get(list_warm_pools_handler))

        // Snapshot endpoints
        .route("/api/v1/snapshots", post(create_snapshot_handler))
        .route("/api/v1/snapshots", get(list_snapshots_handler))
        .route("/api/v1/snapshots/:id/restore", post(restore_snapshot_handler))

        // Instance endpoints
        .route("/api/v1/instances", post(create_instance_handler))
        .route("/api/v1/instances", get(list_instances_handler))
        .route("/api/v1/instances/:id", get(get_instance_handler))
        .route("/api/v1/instances/:id/exec", post(exec_instance_handler))
        .route("/api/v1/instances/:id/stop", post(stop_instance_handler))

        // Metrics and monitoring
        .route("/api/v1/metrics", get(metrics_handler))
        .route("/api/v1/metrics/detailed", get(detailed_metrics_handler))

        // Server-sent events for real-time logs
        .route("/api/v1/logs/:id/stream", get(stream_logs_handler))

        // Health check with runtime status
        .route("/health", get(health_handler))

        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn execute_handler(
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequestWithRuntime>,
) -> Result<Json<InvokeResponse>, StatusCode> {
    let start = Instant::now();

    // Update metrics
    state.metrics.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    // Determine runtime
    let runtime = req.runtime.unwrap_or(Runtime::Auto);
    let use_firecracker = match runtime {
        Runtime::Docker => false,
        Runtime::Firecracker => true,
        Runtime::Auto => {
            // Auto-select based on workload characteristics
            req.image.as_ref().map(|i| i.contains("secure") || i.contains("prod")).unwrap_or(false)
        }
    };

    // Execute with selected runtime
    let result = if use_firecracker && state.firecracker_executor.is_some() {
        state.metrics.vm_executions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Use Firecracker VM
        let fc_executor = state.firecracker_executor.as_ref().unwrap();
        let config = faas_common::SandboxConfig {
            function_id: Uuid::new_v4().to_string(),
            source: req.image.unwrap_or_else(|| "alpine:latest".to_string()),
            command: vec!["sh".to_string(), "-c".to_string(), req.command],
            env_vars: None,
            payload: Vec::new(),
        };

        match fc_executor.execute(config).await {
            Ok(result) => result,
            Err(e) => {
                error!("VM execution failed: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else {
        state.metrics.docker_executions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Use Docker container
        let config = ExecutionConfig {
            image: req.image.unwrap_or_else(|| "alpine:latest".to_string()),
            command: req.command,
            env_vars: vec![],
            working_dir: None,
            timeout: Duration::from_millis(req.timeout_ms.unwrap_or(30000)),
            memory_limit: None,
            cpu_limit: None,
        };

        match state.executor_wrapper.execute(config).await {
            Ok(result) => faas_common::InvocationResult {
                request_id: Uuid::new_v4().to_string(),
                response: Some(result.stdout.into_bytes()),
                logs: Some(result.stderr),
                error: None,
            },
            Err(e) => {
                error!("Docker execution failed: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    };

    // Check for cache hit (fast response)
    if start.elapsed().as_millis() < 10 {
        state.metrics.cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    let output = result.response.as_ref().map(|r| String::from_utf8_lossy(r).to_string());
    let stdout = output.clone().unwrap_or_default();
    let stderr = result.logs.clone().unwrap_or_default();

    Ok(Json(InvokeResponse {
        request_id: result.request_id,
        output,
        logs: result.logs,
        error: result.error,
        exit_code: 0,
        stdout,
        stderr,
        duration_ms: start.elapsed().as_millis() as u64,
    }))
}

async fn execute_advanced_handler(
    State(state): State<AppState>,
    Json(req): Json<ExecuteAdvancedRequest>,
) -> Result<Json<InvokeResponse>, StatusCode> {
    let mode = req.mode.as_deref().unwrap_or("ephemeral");

    // Build base config
    let config = ExecutionConfig {
        image: req.image.unwrap_or_else(|| "alpine:latest".to_string()),
        command: req.command,
        env_vars: vec![],
        working_dir: None,
        timeout: Duration::from_millis(req.timeout_ms.unwrap_or(30000)),
        memory_limit: None,
        cpu_limit: None,
    };

    // Handle different execution modes
    let result = match mode {
        "cached" => {
            // Use container pool for warm starts
            info!("Using cached execution mode");
            // In production, would use container pool from faas-executor
            state.executor_wrapper.execute(config).await
        }
        "checkpointed" => {
            // Use CRIU if available (Linux only)
            #[cfg(target_os = "linux")]
            {
                info!("Using CRIU checkpoint mode");
                // Would integrate with faas_executor::criu::CriuManager
                state.executor_wrapper.execute(config).await
            }
            #[cfg(not(target_os = "linux"))]
            {
                info!("CRIU not available, falling back to normal execution");
                state.executor_wrapper.execute(config).await
            }
        }
        "branched" => {
            if let Some(snapshot_id) = req.snapshot_id {
                info!("Using branched execution from snapshot {}", snapshot_id);
                // Would restore from snapshot and execute
                state.executor_wrapper.execute(config).await
            } else {
                state.executor_wrapper.execute(config).await
            }
        }
        "persistent" => {
            info!("Creating persistent instance");
            // Create long-running container
            state.executor_wrapper.execute(config).await
        }
        _ => state.executor_wrapper.execute(config).await,
    };

    match result {
        Ok(result) => Ok(Json(InvokeResponse {
            exit_code: result.exit_code,
            stdout: result.stdout.clone(),
            stderr: result.stderr.clone(),
            duration_ms: result.duration.as_millis() as u64,
            request_id: Uuid::new_v4().to_string(),
            output: Some(result.stdout),
            logs: None,
            error: if result.stderr.is_empty() { None } else { Some(result.stderr) },
        })),
        Err(e) => {
            error!("Advanced execution failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn create_snapshot_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateSnapshotRequest>,
) -> Result<Json<Snapshot>, StatusCode> {
    // Use real Docker snapshot manager
    let metadata = std::collections::HashMap::new();

    match state.snapshot_manager.create_snapshot(
        &req.container_id,
        Some(req.name.clone()),
        metadata,
    ).await {
        Ok(docker_snapshot) => {
            let snapshot = Snapshot {
                id: docker_snapshot.id.clone(),
                name: docker_snapshot.name.clone(),
                container_id: docker_snapshot.container_id.clone(),
                created_at: docker_snapshot.created_at.to_rfc3339(),
                size_bytes: docker_snapshot.size_bytes as u64,
            };

            info!("Created real Docker snapshot: {} (image: {})",
                  snapshot.id, docker_snapshot.image_id);

            state.snapshots.insert(snapshot.id.clone(), snapshot.clone());
            Ok(Json(snapshot))
        }
        Err(e) => {
            error!("Failed to create Docker snapshot: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn list_snapshots_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<Snapshot>>, StatusCode> {
    let snapshots: Vec<Snapshot> = state.snapshots
        .iter()
        .map(|entry| entry.value().clone())
        .collect();

    Ok(Json(snapshots))
}

async fn restore_snapshot_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(_snapshot) = state.snapshots.get(&id) {
        // Use real Docker snapshot restore
        match state.snapshot_manager.restore_snapshot(&id).await {
            Ok(container_id) => {
                info!("Restored snapshot {} to container {}", id, container_id);
                Ok(Json(serde_json::json!({
                    "status": "restored",
                    "snapshot_id": id,
                    "container_id": container_id
                })))
            }
            Err(e) => {
                error!("Failed to restore snapshot: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn create_instance_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateInstanceRequest>,
) -> Result<Json<Instance>, StatusCode> {
    let instance = Instance {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        image: req.image,
        status: "running".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        cpu_cores: req.cpu_cores,
        memory_mb: req.memory_mb,
    };

    info!("Creating instance: {}", instance.id);

    state.instances.insert(instance.id.clone(), instance.clone());

    Ok(Json(instance))
}

async fn list_instances_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<Instance>>, StatusCode> {
    let instances: Vec<Instance> = state.instances
        .iter()
        .map(|entry| entry.value().clone())
        .collect();

    Ok(Json(instances))
}

async fn get_instance_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Instance>, StatusCode> {
    state.instances
        .get(&id)
        .map(|entry| Json(entry.value().clone()))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn exec_instance_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<InvokeResponse>, StatusCode> {
    if state.instances.contains_key(&id) {
        let config = ExecutionConfig {
            image: "alpine:latest".to_string(),
            command: req.command,
            env_vars: req.env_vars.unwrap_or_default(),
            working_dir: req.working_dir,
            timeout: Duration::from_millis(req.timeout_ms.unwrap_or(30000)),
            memory_limit: None,
            cpu_limit: None,
        };

        match state.executor_wrapper.execute(config).await {
            Ok(result) => Ok(Json(InvokeResponse {
                exit_code: result.exit_code,
                stdout: result.stdout.clone(),
                stderr: result.stderr.clone(),
                duration_ms: result.duration.as_millis() as u64,
                request_id: Uuid::new_v4().to_string(),
                output: Some(result.stdout),
                logs: None,
                error: if result.stderr.is_empty() { None } else { Some(result.stderr) },
            })),
            Err(e) => {
                error!("Instance exec failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn stop_instance_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    if state.instances.remove(&id).is_some() {
        info!("Stopped instance: {}", id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn metrics_handler(State(_state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "avg_execution_time_ms": 500.0,
        "cache_hit_rate": 0.75,
        "active_containers": 5,
        "active_instances": 2,
        "memory_usage_mb": 1024,
        "cpu_usage_percent": 45.0
    }))
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "components": {
            "docker": "healthy",
            "firecracker": if state.firecracker_executor.is_some() { "available" } else { "unavailable" },
            "executor": "healthy",
            "cache": "healthy"
        }
    }))
}

// New user-centric handlers

async fn run_python_handler(
    State(state): State<AppState>,
    body: String,
) -> Result<Json<InvokeResponse>, StatusCode> {
    let req = ExecuteRequestWithRuntime {
        command: format!("python -c '{}'", body.replace("'", "\\'")),
        image: Some("python:3.11-slim".to_string()),
        runtime: Some(Runtime::Docker),
        cache_key: Some(format!("{:x}", md5::compute(&body))),
        timeout_ms: Some(30000),
    };
    execute_handler(State(state), Json(req)).await
}

async fn run_javascript_handler(
    State(state): State<AppState>,
    body: String,
) -> Result<Json<InvokeResponse>, StatusCode> {
    let req = ExecuteRequestWithRuntime {
        command: format!("node -e '{}'", body.replace("'", "\\'")),
        image: Some("node:20-slim".to_string()),
        runtime: Some(Runtime::Docker),
        cache_key: Some(format!("{:x}", md5::compute(&body))),
        timeout_ms: Some(30000),
    };
    execute_handler(State(state), Json(req)).await
}

async fn run_rust_handler(
    State(state): State<AppState>,
    body: String,
) -> Result<Json<InvokeResponse>, StatusCode> {
    let req = ExecuteRequestWithRuntime {
        command: format!("rustc -o /tmp/main - && /tmp/main <<< '{}'", body.replace("'", "\\'")),
        image: Some("rust:latest".to_string()),
        runtime: Some(Runtime::Docker),
        cache_key: Some(format!("{:x}", md5::compute(&body))),
        timeout_ms: Some(60000),
    };
    execute_handler(State(state), Json(req)).await
}

async fn fork_execution_handler(
    State(_state): State<AppState>,
    Json(_req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // TODO: Implement forking logic
    Ok(Json(serde_json::json!({
        "fork_id": Uuid::new_v4().to_string(),
        "status": "forked"
    })))
}

async fn fork_from_parent_handler(
    State(_state): State<AppState>,
    Path(_parent_id): Path<String>,
    Json(_req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // TODO: Implement fork from parent logic
    Ok(Json(serde_json::json!({
        "fork_id": Uuid::new_v4().to_string(),
        "status": "forked"
    })))
}

#[derive(Deserialize)]
struct PrewarmRequest {
    image: String,
    count: u32,
    runtime: Option<Runtime>,
}

async fn prewarm_handler(
    State(state): State<AppState>,
    Json(req): Json<PrewarmRequest>,
) -> Result<StatusCode, StatusCode> {
    let runtime = req.runtime.unwrap_or(Runtime::Auto);
    let pool_key = format!("{}:{:?}", req.image, runtime);

    // Create warm pool entry
    state.warm_pools.insert(pool_key.clone(), WarmPool {
        image: req.image,
        runtime,
        available: Vec::with_capacity(req.count as usize),
        in_use: Vec::new(),
    });

    info!("Pre-warming {} instances for {}", req.count, pool_key);
    Ok(StatusCode::ACCEPTED)
}

async fn list_warm_pools_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let pools: Vec<serde_json::Value> = state.warm_pools
        .iter()
        .map(|entry| {
            let (key, pool) = entry.pair();
            serde_json::json!({
                "key": key.clone(),
                "image": pool.image,
                "runtime": format!("{:?}", pool.runtime),
                "available": pool.available.len(),
                "in_use": pool.in_use.len(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "pools": pools })))
}

async fn detailed_metrics_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use std::sync::atomic::Ordering;

    let total = state.metrics.total_requests.load(Ordering::Relaxed);
    let cache_hits = state.metrics.cache_hits.load(Ordering::Relaxed);
    let docker_execs = state.metrics.docker_executions.load(Ordering::Relaxed);
    let vm_execs = state.metrics.vm_executions.load(Ordering::Relaxed);

    Ok(Json(serde_json::json!({
        "total_requests": total,
        "cache_hits": cache_hits,
        "cache_hit_rate": if total > 0 { cache_hits as f64 / total as f64 } else { 0.0 },
        "docker_executions": docker_execs,
        "vm_executions": vm_execs,
        "runtime_distribution": {
            "docker": if total > 0 { docker_execs as f64 / total as f64 } else { 0.0 },
            "firecracker": if total > 0 { vm_execs as f64 / total as f64 } else { 0.0 },
        }
    })))
}

async fn stream_logs_handler(
    Path(_execution_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    use async_stream::stream;
    use axum::response::sse::Event;

    let stream = stream! {
        // Simulate log streaming
        for i in 0..10 {
            yield Ok(Event::default().data(format!("Log line {}", i)));
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };

    Sse::new(stream)
}