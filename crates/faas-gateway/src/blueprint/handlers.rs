//! HTTP endpoint handlers for Blueprint SDK integration

use super::backend::{BackendType, ExecutionBackend, FaasConfig};
use super::BackendRouter;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{error, info, instrument};

/// Shared application state
pub struct AppState {
    pub router: Arc<BackendRouter>,
}

/// Deploy function request (binary in body, config in header)
#[instrument(skip(state, binary))]
pub async fn deploy_function(
    State(state): State<Arc<AppState>>,
    Path(function_id): Path<String>,
    headers: HeaderMap,
    binary: Bytes,
) -> Result<Json<DeployResponse>, AppError> {
    info!("Deploying function: {}", function_id);

    // Parse config from header
    let config = if let Some(config_header) = headers.get("X-Blueprint-Config") {
        let config_b64 = config_header
            .to_str()
            .map_err(|_| AppError::BadRequest("Invalid config header encoding".to_string()))?;

        let config_json = base64::decode(config_b64)
            .map_err(|_| AppError::BadRequest("Invalid base64 in config header".to_string()))?;

        serde_json::from_slice::<FaasConfig>(&config_json)
            .map_err(|e| AppError::BadRequest(format!("Invalid config JSON: {}", e)))?
    } else {
        FaasConfig::default()
    };

    // Select backend
    let backend_type = parse_backend_header(&headers);
    let backend = state.router.get_backend(backend_type);

    // Deploy
    let info = backend
        .deploy(function_id.clone(), binary.to_vec(), config)
        .await
        .map_err(AppError::from)?;

    Ok(Json(DeployResponse {
        function_id: info.function_id,
        endpoint: info.endpoint,
        status: info.status,
        cold_start_ms: info.cold_start_ms,
        memory_mb: info.memory_mb,
        timeout_secs: info.timeout_secs,
    }))
}

/// Invoke function
#[instrument(skip(state, payload))]
pub async fn invoke_function(
    State(state): State<Arc<AppState>>,
    Path(function_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<InvokeRequest>,
) -> Result<Json<InvokeResponse>, AppError> {
    info!("Invoking function: {}", function_id);

    // Select backend
    let backend_type = parse_backend_header(&headers);
    let backend = state.router.get_backend(backend_type);

    // Serialize payload to JSON bytes
    let payload_json = serde_json::to_vec(&payload)
        .map_err(|e| AppError::BadRequest(format!("Failed to serialize payload: {}", e)))?;

    // Invoke
    let result = backend
        .invoke(function_id, payload_json)
        .await
        .map_err(AppError::from)?;

    Ok(Json(InvokeResponse {
        job_id: result.job_id,
        result: result.result,
        success: result.success,
        execution_ms: result.execution_ms,
        memory_used_mb: 0, // TODO: Track actual memory usage
    }))
}

/// Health check
#[instrument(skip(state))]
pub async fn health_check(
    State(state): State<Arc<AppState>>,
    Path(function_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<HealthResponse>, AppError> {
    let backend_type = parse_backend_header(&headers);
    let backend = state.router.get_backend(backend_type);

    let health = backend
        .health(function_id)
        .await
        .map_err(AppError::from)?;

    Ok(Json(HealthResponse {
        function_id: health.function_id,
        status: health.status,
        last_invocation: health.last_invocation,
        total_invocations: health.total_invocations,
    }))
}

/// Get function info
#[instrument(skip(state))]
pub async fn get_function_info(
    State(state): State<Arc<AppState>>,
    Path(function_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<DeployResponse>, AppError> {
    let backend_type = parse_backend_header(&headers);
    let backend = state.router.get_backend(backend_type);

    let info = backend.info(function_id).await.map_err(AppError::from)?;

    Ok(Json(DeployResponse {
        function_id: info.function_id,
        endpoint: info.endpoint,
        status: info.status,
        cold_start_ms: info.cold_start_ms,
        memory_mb: info.memory_mb,
        timeout_secs: info.timeout_secs,
    }))
}

/// Undeploy function
#[instrument(skip(state))]
pub async fn undeploy_function(
    State(state): State<Arc<AppState>>,
    Path(function_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<UndeployResponse>, AppError> {
    info!("Undeploying function: {}", function_id);

    let backend_type = parse_backend_header(&headers);
    let backend = state.router.get_backend(backend_type);

    backend
        .undeploy(function_id.clone())
        .await
        .map_err(AppError::from)?;

    Ok(Json(UndeployResponse {
        function_id,
        status: "deleted".to_string(),
    }))
}

/// Warm function
#[instrument(skip(state))]
pub async fn warm_function(
    State(state): State<Arc<AppState>>,
    Path(function_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<WarmResponse>, AppError> {
    info!("Warming function: {}", function_id);

    let backend_type = parse_backend_header(&headers);
    let backend = state.router.get_backend(backend_type);

    let instances = backend
        .warm(function_id.clone())
        .await
        .map_err(AppError::from)?;

    Ok(Json(WarmResponse {
        function_id,
        status: "warm".to_string(),
        instances_warmed: instances,
    }))
}

/// Parse backend type from headers
fn parse_backend_header(headers: &HeaderMap) -> Option<BackendType> {
    headers
        .get("X-Execution-Backend")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| BackendType::from_str(s).ok())
}

/// Create router with all blueprint endpoints
pub fn blueprint_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/blueprint/functions/:id", put(deploy_function))
        .route("/api/blueprint/functions/:id", get(get_function_info))
        .route("/api/blueprint/functions/:id", delete(undeploy_function))
        .route("/api/blueprint/functions/:id/invoke", post(invoke_function))
        .route("/api/blueprint/functions/:id/health", get(health_check))
        .route("/api/blueprint/functions/:id/warm", post(warm_function))
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct DeployResponse {
    pub function_id: String,
    pub endpoint: String,
    pub status: String,
    pub cold_start_ms: u64,
    pub memory_mb: u32,
    pub timeout_secs: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InvokeRequest {
    pub job_id: u64,
    pub args: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct InvokeResponse {
    pub job_id: u64,
    pub result: Vec<u8>,
    pub success: bool,
    pub execution_ms: u64,
    pub memory_used_mb: u64,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub function_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_invocation: Option<String>,
    pub total_invocations: u64,
}

#[derive(Debug, Serialize)]
pub struct UndeployResponse {
    pub function_id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct WarmResponse {
    pub function_id: String,
    pub status: String,
    pub instances_warmed: u32,
}

// ============================================================================
// Error Handling
// ============================================================================

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    NotFound(String),
    Internal(String),
}

impl From<super::backend::BackendError> for AppError {
    fn from(err: super::backend::BackendError) -> Self {
        use super::backend::BackendError;
        match err {
            BackendError::NotFound(id) => AppError::NotFound(id),
            BackendError::AlreadyExists(id) => {
                AppError::BadRequest(format!("Function already exists: {}", id))
            }
            BackendError::DeploymentFailed(msg) => AppError::Internal(msg),
            BackendError::ExecutionFailed(msg) => AppError::Internal(msg),
            BackendError::Storage(msg) => AppError::Internal(msg),
            BackendError::Timeout(secs) => {
                AppError::Internal(format!("Timeout after {} seconds", secs))
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, format!("Function not found: {}", msg)),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = serde_json::json!({
            "error": message
        });

        (status, Json(body)).into_response()
    }
}
