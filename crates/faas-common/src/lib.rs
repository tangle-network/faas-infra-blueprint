// Re-export dependencies used in public interfaces of common types

use std::fmt::Display;

use async_trait::async_trait;
pub use serde::{Deserialize, Serialize};
use thiserror::Error;
pub use tracing;
pub use uuid;

#[derive(Error, Debug)]
pub enum FaasError {
    #[error("Executor Error: {0}")]
    Executor(String),

    #[error("Orchestration Error: {0}")]
    Orchestration(String),

    #[error("Gateway Error: {0}")]
    Gateway(String),

    #[error("Configuration Error: {0}")]
    Config(String),

    #[error("Function Definition Invalid: {0}")]
    DefinitionInvalid(String),

    #[error("Resource Not Found: {0}")]
    NotFound(String),

    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal Error: {0}")]
    Internal(String),
}

// Define the primary Result type for FaaS operations
pub type Result<T> = std::result::Result<T, FaasError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Language {
    Python,
    Node,
    Rust,
    Go,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub language: Language,
    pub code_base64: Option<String>,
    pub handler: Option<String>,
    pub dependencies: Option<String>,
    pub memory_mb: Option<u32>,
    pub timeout_sec: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationRequest {
    pub function_id: String,
    pub request_id: String,
    pub payload: Vec<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvocationResult {
    pub request_id: String,
    pub response: Option<Vec<u8>>,
    pub logs: Option<String>,
    pub error: Option<String>,
}

impl Display for InvocationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "InvocationResult(request_id: {}, response: {:?}, logs: {:?}, error: {:?})",
            self.request_id, self.response, self.logs, self.error
        )
    }
}

/// Input arguments for the ExecuteFunction Tangle job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecuteFunctionArgs {
    pub image: String,
    pub command: Vec<String>,
    pub env_vars: Option<Vec<String>>,
    pub payload: Vec<u8>,
}

// Configuration for a sandbox execution request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SandboxConfig {
    pub function_id: String,
    pub source: String,
    pub command: Vec<String>,
    pub env_vars: Option<Vec<String>>,
    pub payload: Vec<u8>,
}

// Define the SandboxExecutor trait
#[async_trait]
pub trait SandboxExecutor: Send + Sync {
    async fn execute(&self, config: SandboxConfig) -> Result<InvocationResult>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json; // Import for test

    #[test]
    fn test_serialization() {
        let def = FunctionDefinition {
            name: "my_func".to_string(),
            language: Language::Python,
            code_base64: Some("cHJpbnQoJ2hlbGxvJyk=".to_string()), // print('hello')
            handler: Some("main.handler".to_string()),
            dependencies: Some("requests".to_string()),
            memory_mb: Some(128),
            timeout_sec: Some(30),
        };
        let json = serde_json::to_string(&def).unwrap();
        println!("{}", json);
        assert!(json.contains("Python"));

        let req = InvocationRequest {
            function_id: "f1".to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            payload: vec![1, 2, 3],
        };
        let json_req = serde_json::to_string(&req).unwrap();
        println!("{}", json_req);
        assert!(json_req.contains("f1"));
    }
}
