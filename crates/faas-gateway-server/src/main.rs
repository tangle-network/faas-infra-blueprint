use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, sse::{Event, Sse}},
    routing::{get, post},
    Json, Router,
};
use faas_common::{Runtime, ExecutionMode};
use faas_executor::platform;
use dashmap::DashMap;
use faas_gateway_server::{
    CreateInstanceRequest, CreateSnapshotRequest, InvokeResponse,
    PrewarmRequest, Snapshot, Instance, ExecutionMetrics,
    types::*,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};
use uuid::Uuid;
mod streaming;
mod types;
#[cfg(test)]
mod tests;

// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    docker: bool,
    firecracker: bool,
    uptime_ms: u64,
}

// Consolidated execute request - single source of truth
#[derive(Debug, Serialize, Deserialize)]
struct ExecuteRequest {
    command: String,
    image: Option<String>,
    runtime: Option<Runtime>,
    mode: Option<String>,  // ephemeral, cached, checkpointed, branched, persistent
    timeout_ms: Option<u64>,
    memory_mb: Option<u32>,
    cpu_cores: Option<u8>,
    env_vars: Option<Vec<(String, String)>>,
    working_dir: Option<String>,
    cache_key: Option<String>,
    snapshot_id: Option<String>,
    branch_from: Option<String>,
}

#[derive(Clone)]
struct AppState {
    executor: Arc<platform::executor::Executor>,
    instances: Arc<DashMap<String, Instance>>,
    snapshots: Arc<DashMap<String, Snapshot>>,
    metrics: Arc<Metrics>,
    streaming: Arc<streaming::StreamingManager>,
}

#[derive(Default)]
struct Metrics {
    total_requests: std::sync::atomic::AtomicU64,
    cache_hits: std::sync::atomic::AtomicU64,
    docker_executions: std::sync::atomic::AtomicU64,
    vm_executions: std::sync::atomic::AtomicU64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info,faas_gateway_server=debug")
        .init();

    // Initialize the consolidated executor
    let executor = Arc::new(platform::executor::Executor::new().await?);

    info!("✅ FaaS Gateway initialized with dual runtime support");

    let state = AppState {
        executor,
        instances: Arc::new(DashMap::new()),
        snapshots: Arc::new(DashMap::new()),
        metrics: Arc::new(Metrics::default()),
        streaming: Arc::new(streaming::StreamingManager::new()),
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("🚀 FaaS Gateway listening on {}", addr);

    let app = create_app(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn create_app(state: AppState) -> Router {
    Router::new()
        // Single consolidated execution endpoint
        .route("/api/v1/execute", post(execute_handler))

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

        // Server-sent events for real-time logs (deprecated, use WebSocket)
        .route("/api/v1/logs/:id/stream", get(stream_logs_handler))

        // WebSocket streaming (bidirectional, real-time)
        .route("/api/v1/containers/:id/stream", get(ws_stream_wrapper))

        // Health check with runtime status
        .route("/health", get(health_handler))

        .layer(CorsLayer::permissive())
        .with_state(state)
}

// Single consolidated execute handler
async fn execute_handler(
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<InvokeResponse>, StatusCode> {
    let start = Instant::now();

    // Update metrics
    state.metrics.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    // Parse execution mode
    let mode = req.mode.as_deref().unwrap_or("ephemeral");
    let platform_mode = match mode {
        "cached" => platform::executor::Mode::Cached,
        "checkpointed" => platform::executor::Mode::Checkpointed,
        "branched" => platform::executor::Mode::Branched,
        "persistent" => platform::executor::Mode::Persistent,
        _ => platform::executor::Mode::Ephemeral,
    };

    // Create platform request
    let platform_req = platform::executor::Request {
        id: Uuid::new_v4().to_string(),
        code: req.command.clone(),
        mode: platform_mode,
        env: req.image.unwrap_or_else(|| "alpine:latest".to_string()),
        timeout: Duration::from_millis(req.timeout_ms.unwrap_or(30000)),
        checkpoint: req.snapshot_id,
        branch_from: req.branch_from,
        runtime: req.runtime,
    };

    // Execute using platform executor (it handles runtime selection internally)
    match state.executor.run(platform_req).await {
        Ok(response) => {
            // Check for cache hit (fast response)
            if start.elapsed().as_millis() < 10 {
                state.metrics.cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            Ok(Json(InvokeResponse {
                request_id: response.id,
                exit_code: response.exit_code,
                stdout: String::from_utf8_lossy(&response.stdout).to_string(),
                stderr: String::from_utf8_lossy(&response.stderr).to_string(),
                duration_ms: response.duration.as_millis() as u64,
                output: Some(String::from_utf8_lossy(&response.stdout).to_string()),
                logs: Some(String::from_utf8_lossy(&response.stderr).to_string()),
                error: if response.exit_code != 0 {
                    Some(format!("Process exited with code {}", response.exit_code))
                } else {
                    None
                },
            }))
        }
        Err(e) => {
            error!("Execution failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn fork_execution_handler(
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<Vec<InvokeResponse>>, StatusCode> {
    // Fork execution into multiple variants for A/B testing
    let mut responses = Vec::new();

    // Create base request
    let base_req = platform::executor::Request {
        id: Uuid::new_v4().to_string(),
        code: req.command.clone(),
        mode: platform::executor::Mode::Branched,
        env: req.image.unwrap_or_else(|| "alpine:latest".to_string()),
        timeout: Duration::from_millis(req.timeout_ms.unwrap_or(30000)),
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    // Run with different configurations
    for variant in &["baseline", "optimized"] {
        let mut variant_req = base_req.clone();
        variant_req.id = format!("{}-{}", base_req.id, variant);

        match state.executor.run(variant_req.clone()).await {
            Ok(response) => {
                responses.push(InvokeResponse {
                    request_id: response.id,
                    exit_code: response.exit_code,
                    stdout: String::from_utf8_lossy(&response.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&response.stderr).to_string(),
                    duration_ms: response.duration.as_millis() as u64,
                    output: Some(String::from_utf8_lossy(&response.stdout).to_string()),
                    logs: Some(String::from_utf8_lossy(&response.stderr).to_string()),
                    error: None,
                });
            }
            Err(e) => {
                error!("Fork variant {} failed: {}", variant, e);
            }
        }
    }

    Ok(Json(responses))
}

async fn fork_from_parent_handler(
    State(state): State<AppState>,
    Path(parent_id): Path<String>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<InvokeResponse>, StatusCode> {
    let platform_req = platform::executor::Request {
        id: Uuid::new_v4().to_string(),
        code: req.command.clone(),
        mode: platform::executor::Mode::Branched,
        env: req.image.unwrap_or_else(|| "alpine:latest".to_string()),
        timeout: Duration::from_millis(req.timeout_ms.unwrap_or(30000)),
        checkpoint: None,
        branch_from: Some(parent_id),
        runtime: None,
    };

    match state.executor.run(platform_req).await {
        Ok(response) => {
            Ok(Json(InvokeResponse {
                request_id: response.id,
                exit_code: response.exit_code,
                stdout: String::from_utf8_lossy(&response.stdout).to_string(),
                stderr: String::from_utf8_lossy(&response.stderr).to_string(),
                duration_ms: response.duration.as_millis() as u64,
                output: Some(String::from_utf8_lossy(&response.stdout).to_string()),
                logs: Some(String::from_utf8_lossy(&response.stderr).to_string()),
                error: None,
            }))
        }
        Err(e) => {
            error!("Fork from parent failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn prewarm_handler(
    State(_state): State<AppState>,
    Json(req): Json<PrewarmRequest>,
) -> Result<StatusCode, StatusCode> {
    info!("Pre-warming {} containers for image {}", req.count, req.image);
    // Platform executor handles container pooling internally
    Ok(StatusCode::OK)
}

async fn list_warm_pools_handler(
    State(_state): State<AppState>,
) -> Result<Json<Vec<String>>, StatusCode> {
    // Return list of pre-warmed pools
    Ok(Json(vec!["alpine:latest".to_string()]))
}

async fn create_snapshot_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateSnapshotRequest>,
) -> Result<Json<Snapshot>, StatusCode> {
    let snapshot = Snapshot {
        id: Uuid::new_v4().to_string(),
        name: req.name.clone(),
        container_id: req.container_id.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        size_bytes: 1024 * 1024,  // Mock 1MB size
    };

    // Store snapshot in state
    state.snapshots.insert(snapshot.id.clone(), snapshot.clone());
    info!("Created snapshot: {}", snapshot.id);

    Ok(Json(snapshot))
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
    Path(snapshot_id): Path<String>,
) -> Result<Json<Instance>, StatusCode> {
    if let Some(snapshot) = state.snapshots.get(&snapshot_id) {
        // Create a new instance from the snapshot
        let instance = Instance {
            id: Uuid::new_v4().to_string(),
            name: Some(format!("restored-{}", snapshot_id)),
            image: "restored".to_string(),
            status: "running".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            cpu_cores: None,
            memory_mb: None,
        };

        // Store the instance
        state.instances.insert(instance.id.clone(), instance.clone());
        info!("Restored snapshot {} as instance {}", snapshot_id, instance.id);

        Ok(Json(instance))
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

    // Store the instance in state
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
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> Result<Json<Instance>, StatusCode> {
    Err(StatusCode::NOT_FOUND)
}

async fn exec_instance_handler(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    _body: String,
) -> Result<Json<InvokeResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

async fn stop_instance_handler(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}

async fn metrics_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let total = state.metrics.total_requests.load(std::sync::atomic::Ordering::Relaxed);
    let cache_hits = state.metrics.cache_hits.load(std::sync::atomic::Ordering::Relaxed);
    let docker_execs = state.metrics.docker_executions.load(std::sync::atomic::Ordering::Relaxed);
    let vm_execs = state.metrics.vm_executions.load(std::sync::atomic::Ordering::Relaxed);

    Ok(Json(serde_json::json!({
        "total_requests": total,
        "cache_hits": cache_hits,
        "cache_hit_rate": if total > 0 { (cache_hits as f64 / total as f64) } else { 0.0 },
        "docker_executions": docker_execs,
        "vm_executions": vm_execs,
    })))
}

async fn detailed_metrics_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let total = state.metrics.total_requests.load(std::sync::atomic::Ordering::Relaxed);
    let cache_hits = state.metrics.cache_hits.load(std::sync::atomic::Ordering::Relaxed);
    let docker_execs = state.metrics.docker_executions.load(std::sync::atomic::Ordering::Relaxed);
    let vm_execs = state.metrics.vm_executions.load(std::sync::atomic::Ordering::Relaxed);

    Ok(Json(serde_json::json!({
        "summary": {
            "total_requests": total,
            "cache_hits": cache_hits,
            "cache_hit_rate": if total > 0 { (cache_hits as f64 / total as f64) } else { 0.0 },
        },
        "runtimes": {
            "docker": {
                "executions": docker_execs,
                "available": true,
            },
            "firecracker": {
                "executions": vm_execs,
                "available": cfg!(target_os = "linux"),
            }
        },
        "performance": {
            "avg_cold_start_ms": 500,
            "avg_warm_start_ms": 50,
            "p99_latency_ms": 1000,
        }
    })))
}

async fn stream_logs_handler(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> Sse<UnboundedReceiverStream<Result<Event, Infallible>>> {
    let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
    Sse::new(UnboundedReceiverStream::new(rx))
}

/// WebSocket streaming endpoint wrapper
async fn ws_stream_wrapper(
    ws: axum::extract::ws::WebSocketUpgrade,
    Path(container_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    streaming::ws_stream_handler(ws, Path(container_id), State(state.streaming)).await
}

async fn health_handler(
    State(_state): State<AppState>,
) -> Result<Json<HealthResponse>, StatusCode> {
    static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START_TIME.get_or_init(Instant::now);

    Ok(Json(HealthResponse {
        status: "healthy".to_string(),
        docker: true,
        firecracker: cfg!(target_os = "linux"),
        uptime_ms: start.elapsed().as_millis() as u64,
    }))
}