use blueprint_sdk::{
    error::Error as SdkError,
    info,
    macros::context::{KeystoreContext, ServicesContext, TangleClientContext},
    runner::config::BlueprintEnvironment,
};
use faas_common::SandboxExecutor;
use faas_executor::{
    docktopus::DockerBuilder, firecracker::FirecrackerExecutor, DockerExecutor, SandboxExecutor,
};
use faas_orchestrator::{Error as OrchestratorError, Orchestrator};
use std::env;
use std::sync::Arc;
use thiserror::Error;

// Error type specifically for context initialization
#[derive(Error, Debug)]
pub enum BlueprintLibError {
    #[error("Orchestration initialization failed: {0}")]
    Orchestration(#[from] OrchestratorError),
    #[error("Blueprint SDK error: {0}")]
    Sdk(#[from] SdkError),
    #[error("Failed to build Docker client: {0}")]
    DockerBuild(String),
    #[error("Executor Initialization Failed: {0}")]
    ExecutorInit(#[from] faas_common::FaasError),
    #[error("Configuration Error: {0}")]
    Config(String),
}

// Derive necessary context traits
#[derive(Clone, TangleClientContext, ServicesContext, KeystoreContext)]
pub struct FaaSContext {
    #[config]
    pub config: BlueprintEnvironment,
    pub orchestrator: Arc<Orchestrator>,
    pub executor_type: String,
}

impl FaaSContext {
    pub async fn new(config: BlueprintEnvironment) -> Result<Self, BlueprintLibError> {
        let executor_type = env::var("FAAS_EXECUTOR_TYPE").unwrap_or_else(|_| "docker".to_string());

        let executor: Arc<dyn SandboxExecutor + Send + Sync> =
            match executor_type.to_lowercase().as_str() {
                "firecracker" => {
                    info!("Initializing Firecracker executor");
                    let fc_bin = env::var("FC_BINARY_PATH").map_err(|_| {
                        BlueprintLibError::Config("FC_BINARY_PATH env var not set".to_string())
                    })?;
                    let kernel_img = env::var("FC_KERNEL_PATH").map_err(|_| {
                        BlueprintLibError::Config("FC_KERNEL_PATH env var not set".to_string())
                    })?;
                    let fc_executor = FirecrackerExecutor::new(fc_bin, kernel_img)?;
                    Arc::new(fc_executor)
                }
                "docker" | _ => {
                    info!("Initializing Docker executor");
                    let builder = DockerBuilder::new()
                        .await
                        .map_err(|e| BlueprintLibError::DockerBuild(e.to_string()))?;
                    let docker_executor = DockerExecutor::new(builder.client());
                    Arc::new(docker_executor)
                }
            };

        let orchestrator = Arc::new(Orchestrator::new(executor));

        Ok(Self {
            config,
            orchestrator,
            executor_type,
        })
    }
}
