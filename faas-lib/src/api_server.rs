use axum::{
    extract::{Json, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use faas_common::{ExecuteFunctionArgs, InvocationResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{error, info, instrument};

use crate::context::FaaSContext;
use crate::jobs::execute_function_job;
use blueprint_sdk::extract::Context;
use blueprint_sdk::tangle::extract::{CallId, TangleArg};

/// API server configuration
#[derive(Clone, Debug)]
pub struct ApiServerConfig {
    pub host: String,
    pub port: u16,
    pub api_keys: HashMap<String, ApiKeyPermissions>,
}

impl Default for ApiServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            api_keys: HashMap::new(),
        }
    }
}

/// API key permissions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiKeyPermissions {
    pub name: String,
    pub can_execute: bool,
    pub can_manage_instances: bool,
    pub rate_limit: Option<u32>, // requests per minute
}

/// Shared state for the API server
#[derive(Clone)]
pub struct ApiState {
    pub context: FaaSContext,
    pub config: ApiServerConfig,
    pub request_counts: Arc<RwLock<HashMap<String, u32>>>, // For rate limiting
}

/// API error response
#[derive(Serialize)]
pub(crate) struct ApiError {
    pub(crate) error: String,
    pub(crate) code: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::BAD_REQUEST, Json(self)).into_response()
    }
}

/// Execute function request (mirrors Tangle job args)
#[derive(Debug, Deserialize, Serialize)]
pub struct ExecuteRequest {
    pub image: String,
    pub command: Vec<String>,
    pub env_vars: Option<Vec<String>>,
    pub payload: Vec<u8>,
    pub mode: Option<String>, // "ephemeral", "cached", "checkpointed", "branched", "persistent"
    pub checkpoint_id: Option<String>,
    pub branch_from: Option<String>,
    pub timeout_secs: Option<u64>,
}

/// Execute function response
#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    pub request_id: String,
    pub response: Option<Vec<u8>>,
    pub logs: Option<String>,
    pub error: Option<String>,
}

impl From<InvocationResult> for ExecuteResponse {
    fn from(result: InvocationResult) -> Self {
        Self {
            request_id: result.request_id,
            response: result.response,
            logs: result.logs,
            error: result.error,
        }
    }
}

/// Instance management endpoints (placeholder for SDK compatibility)
#[derive(Debug, Deserialize)]
pub struct InstanceRequest {
    pub snapshot_id: Option<String>,
    pub resources: ResourceSpec,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResourceSpec {
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub disk_gb: u32,
}

#[derive(Debug, Serialize)]
pub struct InstanceResponse {
    pub id: String,
    pub status: String,
    pub ssh_info: Option<SshInfo>,
}

#[derive(Debug, Serialize)]
pub struct SshInfo {
    pub host: String,
    pub port: u16,
    pub username: String,
}

/// Background service for the API server
pub struct ApiBackgroundService {
    config: ApiServerConfig,
    context: FaaSContext,
}

impl ApiBackgroundService {
    pub fn new(config: ApiServerConfig, context: FaaSContext) -> Self {
        Self { config, context }
    }

    async fn run_server(self) -> Result<(), Box<dyn std::error::Error>> {
        let state = ApiState {
            context: self.context,
            config: self.config.clone(),
            request_counts: Arc::new(RwLock::new(HashMap::new())),
        };

        // Use the comprehensive router from api_routes
        let app = crate::api_routes::build_api_router(state.clone())
            // Add the basic execute endpoint for backward compatibility
            .route(
                "/api/v1/execute",
                post(execute_function_handler).with_state(state.clone()),
            );

        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("API server listening on http://{}", addr);

        axum::serve(listener, app).await?;

        Ok(())
    }
}

impl blueprint_sdk::runner::BackgroundService for ApiBackgroundService {
    fn start(
        &self,
    ) -> impl std::future::Future<Output = Result<tokio::sync::oneshot::Receiver<Result<(), blueprint_sdk::runner::error::RunnerError>>, blueprint_sdk::runner::error::RunnerError>> + Send {
        let config = self.config.clone();
        let context = self.context.clone();

        async move {
            let (tx, rx) = tokio::sync::oneshot::channel();

            tokio::spawn(async move {
                let service = ApiBackgroundService { config, context };
                let result = service.run_server()
                    .await
                    .map_err(|e| blueprint_sdk::runner::error::RunnerError::Other(format!("API server error: {e}").into()));
                let _ = tx.send(result);
            });

            Ok(rx)
        }
    }
}

// Authentication middleware
pub(crate) async fn authenticate(
    headers: &HeaderMap,
    state: &ApiState,
) -> Result<ApiKeyPermissions, ApiError> {
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError {
            error: "Missing API key".to_string(),
            code: "UNAUTHORIZED".to_string(),
        })?;

    state
        .config
        .api_keys
        .get(api_key)
        .cloned()
        .ok_or_else(|| ApiError {
            error: "Invalid API key".to_string(),
            code: "UNAUTHORIZED".to_string(),
        })
}

// Rate limiting check
pub(crate) async fn check_rate_limit(
    api_key: &str,
    limit: Option<u32>,
    state: &ApiState,
) -> Result<(), ApiError> {
    if let Some(limit) = limit {
        let mut counts = state.request_counts.write().await;
        let count = counts.entry(api_key.to_string()).or_insert(0);
        *count += 1;

        if *count > limit {
            return Err(ApiError {
                error: format!("Rate limit exceeded: {limit} requests per minute"),
                code: "RATE_LIMIT_EXCEEDED".to_string(),
            });
        }
    }
    Ok(())
}

// Execute function handler - calls the same logic as Tangle jobs
#[instrument(skip(state, headers))]
async fn execute_function_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, ApiError> {
    // Authenticate
    let permissions = authenticate(&headers, &state).await?;
    if !permissions.can_execute {
        return Err(ApiError {
            error: "Permission denied".to_string(),
            code: "FORBIDDEN".to_string(),
        });
    }

    // Rate limit
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    check_rate_limit(api_key, permissions.rate_limit, &state).await?;

    // Convert to ExecuteFunctionArgs (same as Tangle job)
    let args = ExecuteFunctionArgs {
        image: request.image,
        command: request.command,
        env_vars: request.env_vars,
        payload: request.payload,
    };

    // Call the same job handler that Tangle uses
    let call_id = rand::random::<u64>(); // Generate random call ID for API requests

    match execute_function_job(
        Context(state.context.clone()),
        CallId(call_id),
        TangleArg(args),
    )
    .await
    {
        Ok(result) => Ok(Json(ExecuteResponse {
            request_id: format!("api-{call_id}"),
            response: Some(result.0),
            logs: None,
            error: None,
        })),
        Err(e) => {
            error!("Execution failed: {:?}", e);
            Ok(Json(ExecuteResponse {
                request_id: format!("api-{call_id}"),
                response: None,
                logs: None,
                error: Some(e.to_string()),
            }))
        }
    }
}

// Instance management handlers (placeholder implementations for SDK compatibility)
async fn create_instance_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(_request): Json<InstanceRequest>,
) -> Result<Json<InstanceResponse>, ApiError> {
    let permissions = authenticate(&headers, &state).await?;
    if !permissions.can_manage_instances {
        return Err(ApiError {
            error: "Permission denied".to_string(),
            code: "FORBIDDEN".to_string(),
        });
    }

    // Placeholder: In future, this would create persistent instances
    Ok(Json(InstanceResponse {
        id: format!("inst-{}", uuid::Uuid::new_v4()),
        status: "pending".to_string(),
        ssh_info: None,
    }))
}

async fn get_instance_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<InstanceResponse>, ApiError> {
    let _ = authenticate(&headers, &state).await?;

    // Placeholder
    Ok(Json(InstanceResponse {
        id,
        status: "running".to_string(),
        ssh_info: Some(SshInfo {
            host: "localhost".to_string(),
            port: 22,
            username: "faas".to_string(),
        }),
    }))
}

async fn stop_instance_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<InstanceResponse>, ApiError> {
    let permissions = authenticate(&headers, &state).await?;
    if !permissions.can_manage_instances {
        return Err(ApiError {
            error: "Permission denied".to_string(),
            code: "FORBIDDEN".to_string(),
        });
    }

    // Placeholder
    Ok(Json(InstanceResponse {
        id,
        status: "stopped".to_string(),
        ssh_info: None,
    }))
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "faas-api-server"
    }))
}
