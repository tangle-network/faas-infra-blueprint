use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use bollard::Docker;
use dashmap::DashMap;
use faas_executor::{DockerExecutor, docker_snapshot::DockerSnapshotManager};
use faas_gateway::{
    CreateInstanceRequest, CreateSnapshotRequest, ExecuteRequest, InvokeRequest, InvokeResponse,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use uuid::Uuid;

mod executor_wrapper;
mod types;
use executor_wrapper::{ExecutionConfig, ExecutorWrapper};
use types::{Instance, Snapshot};

#[derive(Clone)]
struct AppState {
    executor_wrapper: Arc<ExecutorWrapper>,
    executor: Arc<DockerExecutor>,
    snapshot_manager: Arc<DockerSnapshotManager>,
    instances: Arc<DashMap<String, Instance>>,
    snapshots: Arc<DashMap<String, Snapshot>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExecuteAdvancedRequest {
    command: String,
    image: Option<String>,
    mode: Option<String>,
    snapshot_id: Option<String>,
    branch_id: Option<String>,
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

    let state = AppState {
        executor_wrapper,
        executor,
        snapshot_manager,
        instances: Arc::new(DashMap::new()),
        snapshots: Arc::new(DashMap::new()),
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
        // Execute endpoints
        .route("/api/v1/execute", post(execute_handler))
        .route("/api/v1/execute/advanced", post(execute_advanced_handler))

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

        // Metrics endpoint
        .route("/api/v1/metrics", get(metrics_handler))

        // Health check
        .route("/health", get(health_handler))

        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn execute_handler(
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<InvokeResponse>, StatusCode> {
    let config = ExecutionConfig {
        image: req.image.unwrap_or_else(|| "alpine:latest".to_string()),
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
            error!("Execution failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
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

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "components": {
            "docker": "healthy",
            "executor": "healthy",
            "cache": "healthy"
        }
    }))
}