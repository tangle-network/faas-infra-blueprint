use faas_common::{
    FaasError, FunctionDefinition, InvocationResult, SandboxConfig, SandboxExecutor,
};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::{error, info, instrument, warn}; // Ensure error is imported for logging

// --- Custom Error Type ---
#[derive(Error, Debug)]
pub enum Error {
    #[error("Executor Error")] // Remove specific message if source is always present
    ExecutorError {
        #[from]
        source: FaasError,
    }, // Now wraps common::FaasError
    #[error("Function not found in registry: {0}")]
    FunctionNotFound(String),
    #[error("Scheduling Failed: {0}")]
    SchedulingFailed(String),
}
pub type Result<T> = std::result::Result<T, Error>;

pub use faas_common as common; // Keep this
                               // pub use faas_common::Executor; // REMOVE this - trait is imported via use faas_common::Executor

// Keep struct definitions as placeholders for future implementation

#[derive(Debug, Default)]
pub struct FunctionRegistry {
    // Using std::sync::Mutex for simplicity, consider tokio::sync::Mutex if async contention is high
    functions: std::sync::Mutex<HashMap<String, FunctionDefinition>>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn register(&self, id: String, def: FunctionDefinition) -> Result<()> {
        info!(%id, name=%def.name, "Registering function");
        let mut funcs = self
            .functions
            .lock()
            .map_err(|_| Error::SchedulingFailed("Function registry lock poisoned".to_string()))?;
        // Basic validation: check if ID already exists?
        if funcs.contains_key(&id) {
            // Handle update or return error?
            error!(%id, "Function ID already exists");
            // For now, let's allow overwrite with a warning
            warn!(%id, "Function ID already exists, overwriting definition.");
        }
        funcs.insert(id, def);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<FunctionDefinition>> {
        info!(%id, "Looking up function");
        let funcs = self
            .functions
            .lock()
            .map_err(|_| Error::SchedulingFailed("Function registry lock poisoned".to_string()))?;
        // Clone the definition if found
        Ok(funcs.get(id).cloned())
    }
}

// --- Orchestrator Implementation ---
#[derive(Clone)]
pub struct Orchestrator {
    executor: Arc<dyn SandboxExecutor + Send + Sync>,
    registry: Arc<FunctionRegistry>, // Add registry
}
impl Orchestrator {
    pub fn new(executor: Arc<dyn SandboxExecutor + Send + Sync>) -> Self {
        Self {
            executor,
            registry: Arc::new(FunctionRegistry::new()), // Initialize registry
        }
    }

    // Placeholder for function registration - Task added to Plan.md Phase 4
    pub async fn register_function(&self, id: String, def: FunctionDefinition) -> Result<()> {
        // Delegate to the registry
        self.registry.register(id, def)
    }

    #[instrument(skip(self, image, command, payload), fields(image=%image))]
    pub async fn schedule_execution(
        &self,
        function_id: String,
        image: String,
        command: Vec<String>,
        env_vars: Option<Vec<String>>,
        payload: Vec<u8>,
    ) -> Result<InvocationResult> {
        // Optional: Could lookup function def from registry here if needed
        // let _def = self.registry.get(&function_id)?.ok_or_else(|| Error::FunctionNotFound(function_id.clone()))?;

        let config = SandboxConfig {
            function_id,
            source: image,
            command,
            env_vars,
            payload,
        };
        self.executor.execute(config).await.map_err(|e| {
            error!(source=%e, "Executor execution failed");
            Error::ExecutorError { source: e }
        })
    }
}

// --- Tests ---
#[cfg(test)]
mod tests {
    use super::{Error as OrchestratorError, Orchestrator};
    use crate::common::SandboxExecutor;
    use faas_common::{FunctionDefinition, Language};
    use faas_executor::docktopus::DockerBuilder;
    use faas_executor::DockerExecutor;
    use std::error::Error as StdError;
    use std::sync::Arc;

    // Helper to create a real DockerExecutor instance for tests
    async fn create_real_executor_arc(
    ) -> Result<Arc<dyn SandboxExecutor + Send + Sync>, OrchestratorError> {
        let docker_builder = DockerBuilder::new().await.map_err(|e| {
            OrchestratorError::SchedulingFailed(format!("Failed to create DockerBuilder: {}", e))
        })?;
        let docker_client = docker_builder.client();
        Ok(Arc::new(DockerExecutor::new(docker_client)))
    }

    // Add a basic test for registration
    #[tokio::test]
    async fn test_register_and_get_function() -> Result<(), anyhow::Error> {
        let executor = create_real_executor_arc().await?;
        let orchestrator = Orchestrator::new(executor);

        let func_id = "test-func-reg".to_string();
        let func_def = FunctionDefinition {
            name: "Test Function".to_string(),
            language: Language::Python, // Assuming Language enum exists in common
            code_base64: None,
            handler: None,
            dependencies: None,
            memory_mb: None,
            timeout_sec: None,
        };

        orchestrator
            .register_function(func_id.clone(), func_def.clone())
            .await?;

        let retrieved_def = orchestrator
            .registry
            .get(&func_id)?
            .expect("Function should be found");
        // Basic check - more thorough checks if fields were complex
        assert_eq!(retrieved_def.name, func_def.name);
        assert_eq!(retrieved_def.language, func_def.language);

        Ok(())
    }

    #[tokio::test]
    async fn test_schedule_execution_success() -> Result<(), anyhow::Error> {
        let executor = create_real_executor_arc().await?;
        let orchestrator = Orchestrator::new(executor);

        let function_id = "test-echo-orch".to_string();
        let image = "alpine:latest".to_string();
        let msg = "Hello via Orchestrator!";
        let command = vec!["echo".to_string(), msg.to_string()];
        let env_vars = None;
        let payload = msg.as_bytes().to_vec();

        let result = orchestrator
            .schedule_execution(function_id, image, command, env_vars, payload)
            .await?;

        assert!(
            result.error.is_none(),
            "Expected no error, got: {:?}",
            result.error
        );
        let expected_output = format!("{}\n", msg);
        assert_eq!(
            result.response.as_deref().unwrap(),
            expected_output.as_bytes()
        );
        assert!(result.logs.unwrap_or_default().contains(msg));
        Ok(())
    }

    #[tokio::test]
    async fn test_schedule_execution_executor_error() -> Result<(), anyhow::Error> {
        let executor = create_real_executor_arc().await?;
        let orchestrator = Orchestrator::new(executor);

        let function_id = "test-error-orch".to_string();
        let image = "alpine:latest".to_string();
        let command = vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo 'stderr orch' >&2; cat; exit 7".to_string(),
        ];
        let env_vars = None;
        let payload = b"some input".to_vec();

        let result = orchestrator
            .schedule_execution(function_id, image, command, env_vars, payload)
            .await?;

        assert!(result.response.is_none(), "Expected no response on error");
        assert!(result.error.is_some(), "Expected an error message");
        let error_msg = result.error.unwrap();
        let expected_error_part = "Container failed with exit code: 7";
        assert!(
            error_msg.contains(expected_error_part),
            "Error message mismatch: Expected to contain '{}', got: '{}'",
            expected_error_part,
            error_msg
        );
        assert!(result.logs.unwrap_or_default().contains("stderr orch"));
        Ok(())
    }

    #[tokio::test]
    async fn test_orchestrator_schedule_image_not_found() -> Result<(), anyhow::Error> {
        let executor = create_real_executor_arc().await?;
        let orchestrator = Orchestrator::new(executor);

        let function_id = "test-img-not-found-orch".to_string();
        let image =
            "docker.io/library/this-image-definitely-does-not-exist-ever:latest".to_string();
        let command = vec!["echo".to_string(), "hello".to_string()];
        let env_vars = None;
        let payload = Vec::new();

        let result = orchestrator
            .schedule_execution(function_id, image, command, env_vars, payload)
            .await;

        assert!(result.is_err(), "Expected schedule_execution to fail");
        let orchestrator_error = result.err().unwrap();
        assert!(matches!(
            orchestrator_error,
            OrchestratorError::ExecutorError { .. }
        ));
        let source_error_string = orchestrator_error.source().unwrap().to_string();
        assert!(
            source_error_string.contains("Container creation failed")
                && (source_error_string.contains("No such image")
                    || source_error_string.contains("404")),
            "Expected underlying error to indicate image not found, got: {}",
            source_error_string
        );
        Ok(())
    }
}
