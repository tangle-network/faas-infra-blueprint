use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::post,
    Router,
};
use faas_common::{InvocationResult, Language};
use faas_orchestrator::Error as OrchestratorError;
use faas_orchestrator::Orchestrator;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::{error, info, instrument};

// Expose common types if needed directly
pub use faas_common as common;

// Example: Define traits or structs representing the API contract
// These could be used for generating job schemas or validating inputs/outputs

// Placeholder function definition structure (might mirror common::FunctionDefinition)

// Placeholder invocation request structure

// Placeholder invocation response structure

pub fn add_gateway_stuff(left: usize, right: usize) -> usize {
    // Dummy function demonstrating library usage
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add_gateway_stuff(2, 2);
        assert_eq!(result, 4);
    }
}

// --- API Request/Response Structs ---

#[derive(Deserialize, Debug)]
pub struct InvokeRequest {
    image: String,
    command: Vec<String>,
    env_vars: Option<Vec<String>>,
    payload: Vec<u8>,
}

// Use InvocationResult directly for success response body (serialized to JSON)

// --- API Error Handling ---

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Function execution failed: {0}")]
    ExecutionError(String),
    #[error("Bad Request: {0}")]
    BadRequest(String),
    #[error("Internal server error")]
    InternalError,
}

// Implement IntoResponse for ApiError
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::BadRequest(ref msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::ExecutionError(ref msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            ApiError::InternalError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An internal error occurred".to_string(),
            ),
        };
        (status, Json(serde_json::json!({ "error": error_message }))).into_response()
    }
}

// Convert Orchestrator errors to API errors
impl From<OrchestratorError> for ApiError {
    fn from(err: OrchestratorError) -> Self {
        error!(source=%err, "Orchestrator error occurred");
        match err {
            // Include the source error's details in the API message
            OrchestratorError::ExecutorError { source } => {
                // source is FaasError, which displays as "Executor Error: <details>"
                ApiError::ExecutionError(source.to_string())
            }
            _ => ApiError::InternalError,
        }
    }
}

// Type alias for handler results
type ApiResult<T> = std::result::Result<T, ApiError>;

// --- Gateway Service Setup (Axum) ---
pub fn create_axum_router(orchestrator: Arc<Orchestrator>) -> Router {
    Router::new()
        .route("/functions/:id/invoke", post(handle_invoke))
        .with_state(orchestrator)
}

// --- API Handlers ---
#[axum::debug_handler]
#[instrument(skip(orchestrator, req_body), fields(function_id=%function_id))]
async fn handle_invoke(
    State(orchestrator): State<Arc<Orchestrator>>,
    Path(function_id): Path<String>,
    Json(req_body): Json<InvokeRequest>,
) -> ApiResult<Json<InvocationResult>> {
    info!(image=%req_body.image, command=?req_body.command, "Handling invocation request");

    // Call the orchestrator
    let invocation_result = orchestrator
        .schedule_execution(
            function_id,
            req_body.image,
            req_body.command,
            req_body.env_vars,
            req_body.payload,
        )
        .await?;

    // Check if the InvocationResult itself indicates an execution error
    if let Some(err_msg) = &invocation_result.error {
        error!(error_message = %err_msg, logs = ?invocation_result.logs, "Function execution reported error in InvocationResult");
        // Map this internal execution error to an API error response
        Err(ApiError::ExecutionError(err_msg.clone()))
    } else {
        // Only return Ok(Json(...)) if invocation_result.error is None
        Ok(Json(invocation_result))
    }
}

// --- Library Entry Point / Example (Optional) ---
