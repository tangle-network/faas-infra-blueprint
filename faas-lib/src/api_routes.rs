use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::instrument;

use crate::api_server::{authenticate, check_rate_limit, ApiError, ApiState};
use crate::jobs::*;
use blueprint_sdk::extract::Context;
use blueprint_sdk::tangle::extract::{CallId, TangleArg, TangleArgs4, TangleArgs8};

// ============================================================================
// READ-ONLY ENDPOINTS (API Server only - no Tangle jobs needed)
// ============================================================================

/// List all available snapshots
#[derive(Debug, Serialize)]
pub struct SnapshotInfo {
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub size_bytes: u64,
    pub container_id: Option<String>,
}

#[instrument(skip(state, headers))]
pub async fn list_snapshots_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SnapshotInfo>>, ApiError> {
    let _ = authenticate(&headers, &state).await?;

    // TODO: Query actual snapshot store
    let snapshots = vec![SnapshotInfo {
        id: "snap_example_1".to_string(),
        name: "python-env".to_string(),
        created_at: 1700000000,
        size_bytes: 1024 * 1024 * 512, // 512MB
        container_id: Some("container_123".to_string()),
    }];

    Ok(Json(snapshots))
}

/// Get instance information (SSH details, status, etc.)
#[derive(Debug, Serialize)]
pub struct InstanceInfo {
    pub id: String,
    pub status: String, // "running", "stopped", "paused"
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_username: Option<String>,
    pub exposed_ports: HashMap<u16, String>,
    pub created_at: u64,
    pub resources: ResourceInfo,
}

#[derive(Debug, Serialize)]
pub struct ResourceInfo {
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub disk_gb: u32,
}

#[instrument(skip(state, headers))]
pub async fn get_instance_info_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(instance_id): Path<String>,
) -> Result<Json<InstanceInfo>, ApiError> {
    let _ = authenticate(&headers, &state).await?;

    // TODO: Query actual instance registry
    let info = InstanceInfo {
        id: instance_id.clone(),
        status: "running".to_string(),
        ssh_host: Some("ssh.faas.local".to_string()),
        ssh_port: Some(2222),
        ssh_username: Some("faas".to_string()),
        exposed_ports: HashMap::new(),
        created_at: 1700000000,
        resources: ResourceInfo {
            cpu_cores: 2,
            memory_mb: 4096,
            disk_gb: 20,
        },
    };

    Ok(Json(info))
}

/// List all running instances
#[instrument(skip(state, headers))]
pub async fn list_instances_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Vec<InstanceInfo>>, ApiError> {
    let _ = authenticate(&headers, &state).await?;

    // TODO: Query actual instance registry
    let instances = vec![];

    Ok(Json(instances))
}

/// Get execution logs
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub execution_id: String,
    pub tail: Option<usize>,
    pub follow: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct LogsResponse {
    pub logs: Vec<String>,
    pub has_more: bool,
}

#[instrument(skip(state, headers))]
pub async fn get_logs_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<LogsQuery>,
) -> Result<Json<LogsResponse>, ApiError> {
    let _ = authenticate(&headers, &state).await?;

    // TODO: Query actual log storage
    let response = LogsResponse {
        logs: vec!["Example log line".to_string()],
        has_more: false,
    };

    Ok(Json(response))
}

/// Get metrics/usage
#[derive(Debug, Serialize)]
pub struct UsageMetrics {
    pub total_executions: u64,
    pub active_instances: u32,
    pub total_snapshots: u32,
    pub storage_used_gb: f64,
    pub cpu_hours: f64,
}

#[instrument(skip(state, headers))]
pub async fn get_usage_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<UsageMetrics>, ApiError> {
    let _ = authenticate(&headers, &state).await?;

    // TODO: Query actual metrics
    let metrics = UsageMetrics {
        total_executions: 100,
        active_instances: 2,
        total_snapshots: 5,
        storage_used_gb: 10.5,
        cpu_hours: 24.3,
    };

    Ok(Json(metrics))
}

// ============================================================================
// STATE-CHANGING ENDPOINTS (Call corresponding Tangle jobs)
// ============================================================================

/// Execute with advanced options
#[instrument(skip(state, headers))]
pub async fn execute_advanced_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<ExecuteAdvancedArgs>,
) -> Result<Json<ExecuteResponse>, ApiError> {
    let permissions = authenticate(&headers, &state).await?;
    if !permissions.can_execute {
        return Err(ApiError {
            error: "Permission denied".to_string(),
            code: "FORBIDDEN".to_string(),
        });
    }

    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    check_rate_limit(api_key, permissions.rate_limit, &state).await?;

    let call_id = rand::random::<u64>();

    match execute_advanced_job(
        Context(state.context.clone()),
        CallId(call_id),
        TangleArgs8(
            request.image,
            request.command,
            request.env_vars,
            request.payload,
            request.mode,
            request.checkpoint_id,
            request.branch_from,
            request.timeout_secs,
        ),
    )
    .await
    {
        Ok(result) => Ok(Json(ExecuteResponse {
            request_id: format!("api-{call_id}"),
            response: Some(result.0),
            logs: None,
            error: None,
        })),
        Err(e) => Ok(Json(ExecuteResponse {
            request_id: format!("api-{call_id}"),
            response: None,
            logs: None,
            error: Some(e.to_string()),
        })),
    }
}

#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    pub request_id: String,
    pub response: Option<Vec<u8>>,
    pub logs: Option<String>,
    pub error: Option<String>,
}

/// Create snapshot
#[instrument(skip(state, headers))]
pub async fn create_snapshot_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<CreateSnapshotArgs>,
) -> Result<Json<SnapshotResponse>, ApiError> {
    let permissions = authenticate(&headers, &state).await?;
    if !permissions.can_execute {
        return Err(ApiError {
            error: "Permission denied".to_string(),
            code: "FORBIDDEN".to_string(),
        });
    }

    let call_id = rand::random::<u64>();

    match create_snapshot_job(
        Context(state.context.clone()),
        CallId(call_id),
        TangleArg(request),
    )
    .await
    {
        Ok(result) => Ok(Json(SnapshotResponse {
            snapshot_id: result.0,
            error: None,
        })),
        Err(e) => Ok(Json(SnapshotResponse {
            snapshot_id: String::new(),
            error: Some(e.to_string()),
        })),
    }
}

#[derive(Debug, Serialize)]
pub struct SnapshotResponse {
    pub snapshot_id: String,
    pub error: Option<String>,
}

/// Create branch
#[instrument(skip(state, headers))]
pub async fn create_branch_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<CreateBranchArgs>,
) -> Result<Json<BranchResponse>, ApiError> {
    let permissions = authenticate(&headers, &state).await?;
    if !permissions.can_execute {
        return Err(ApiError {
            error: "Permission denied".to_string(),
            code: "FORBIDDEN".to_string(),
        });
    }

    let call_id = rand::random::<u64>();

    match create_branch_job(
        Context(state.context.clone()),
        CallId(call_id),
        TangleArg(request),
    )
    .await
    {
        Ok(result) => Ok(Json(BranchResponse {
            branch_id: result.0,
            error: None,
        })),
        Err(e) => Ok(Json(BranchResponse {
            branch_id: String::new(),
            error: Some(e.to_string()),
        })),
    }
}

#[derive(Debug, Serialize)]
pub struct BranchResponse {
    pub branch_id: String,
    pub error: Option<String>,
}

/// Start instance
#[instrument(skip(state, headers))]
pub async fn start_instance_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<StartInstanceArgs>,
) -> Result<Json<InstanceResponse>, ApiError> {
    let permissions = authenticate(&headers, &state).await?;
    if !permissions.can_manage_instances {
        return Err(ApiError {
            error: "Permission denied".to_string(),
            code: "FORBIDDEN".to_string(),
        });
    }

    let call_id = rand::random::<u64>();

    match start_instance_job(
        Context(state.context.clone()),
        CallId(call_id),
        TangleArg(request),
    )
    .await
    {
        Ok(result) => Ok(Json(InstanceResponse {
            instance_id: result.0,
            status: "starting".to_string(),
            error: None,
        })),
        Err(e) => Ok(Json(InstanceResponse {
            instance_id: String::new(),
            status: "failed".to_string(),
            error: Some(e.to_string()),
        })),
    }
}

#[derive(Debug, Serialize)]
pub struct InstanceResponse {
    pub instance_id: String,
    pub status: String,
    pub error: Option<String>,
}

/// Expose port
#[instrument(skip(state, headers))]
pub async fn expose_port_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<ExposePortArgs>,
) -> Result<Json<PortResponse>, ApiError> {
    let permissions = authenticate(&headers, &state).await?;
    if !permissions.can_manage_instances {
        return Err(ApiError {
            error: "Permission denied".to_string(),
            code: "FORBIDDEN".to_string(),
        });
    }

    let call_id = rand::random::<u64>();

    match expose_port_job(
        Context(state.context.clone()),
        CallId(call_id),
        TangleArg(request),
    )
    .await
    {
        Ok(result) => Ok(Json(PortResponse {
            public_url: result.0,
            error: None,
        })),
        Err(e) => Ok(Json(PortResponse {
            public_url: String::new(),
            error: Some(e.to_string()),
        })),
    }
}

#[derive(Debug, Serialize)]
pub struct PortResponse {
    pub public_url: String,
    pub error: Option<String>,
}

/// Build complete API router
pub fn build_api_router(state: ApiState) -> Router {
    Router::new()
        // === Read-only endpoints (API only) ===
        .route("/api/v1/snapshots", get(list_snapshots_handler))
        .route("/api/v1/instances", get(list_instances_handler))
        .route("/api/v1/instances/:id/info", get(get_instance_info_handler))
        .route("/api/v1/logs", get(get_logs_handler))
        .route("/api/v1/usage", get(get_usage_handler))
        // === State-changing endpoints (call Tangle jobs) ===
        // Execution
        .route("/api/v1/execute/advanced", post(execute_advanced_handler))
        // Snapshots
        .route("/api/v1/snapshots", post(create_snapshot_handler))
        // Branching
        .route("/api/v1/branches", post(create_branch_handler))
        // Instances
        .route("/api/v1/instances", post(start_instance_handler))
        // Ports
        .route("/api/v1/ports/expose", post(expose_port_handler))
        // Health check
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"status": "healthy"})) }),
        )
        .with_state(state)
}
