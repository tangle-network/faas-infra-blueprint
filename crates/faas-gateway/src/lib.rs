// API types for FaaS gateway

use faas_common::InvocationResult;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Internal server error: {0}")]
    Internal(String),
    #[error("Service unavailable")]
    ServiceUnavailable,
}

#[derive(Debug, Deserialize)]
pub struct InvokeRequest {
    pub image: String,
    pub command: Vec<String>,
    pub env_vars: Option<Vec<String>>,
    pub payload: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InvokeResponse {
    pub request_id: String,
    pub output: Option<String>,
    pub logs: Option<String>,
    pub error: Option<String>,
}

impl From<InvocationResult> for InvokeResponse {
    fn from(result: InvocationResult) -> Self {
        Self {
            request_id: result.request_id,
            output: result.response.and_then(|b| String::from_utf8(b).ok()),
            logs: result.logs,
            error: result.error,
        }
    }
}
