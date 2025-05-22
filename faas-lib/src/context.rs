use blueprint_sdk::{
    error::Error as SdkError,
    info,
    macros::context::{KeystoreContext, ServicesContext, TangleClientContext},
    runner::config::BlueprintEnvironment,
};
use faas_common::SandboxExecutor;
use faas_executor::{docktopus::DockerBuilder, firecracker::FirecrackerExecutor, DockerExecutor};
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
    #[error("Missing environment variable: {0}")]
    MissingEnvVar(String),
}

// Derive necessary context traits
#[derive(Clone, TangleClientContext, ServicesContext, KeystoreContext)]
pub struct FaaSContext {
    #[config]
    pub config: BlueprintEnvironment,
    pub orchestrator: Arc<Orchestrator>,
    pub executor_type: String,
    pub default_firecracker_rootfs_path: Option<String>, // For Firecracker executor
}

impl FaaSContext {
    pub async fn new(config: BlueprintEnvironment) -> Result<Self, BlueprintLibError> {
        let executor_type = env::var("FAAS_EXECUTOR_TYPE")
            .unwrap_or_else(|_| "docker".to_string())
            .to_lowercase();
        info!(selected_executor_type = %executor_type, "Initializing FaaS executor");

        let mut default_firecracker_rootfs_path: Option<String> = None;

        let executor: Arc<dyn SandboxExecutor + Send + Sync> = match executor_type.as_str() {
            "firecracker" => {
                let fc_bin = env::var("FAAS_FC_BINARY_PATH").map_err(|_| {
                    BlueprintLibError::MissingEnvVar(
                        "FAAS_FC_BINARY_PATH not set for Firecracker executor".to_string(),
                    )
                })?;
                let kernel_img = env::var("FAAS_FC_KERNEL_PATH").map_err(|_| {
                    BlueprintLibError::MissingEnvVar(
                        "FAAS_FC_KERNEL_PATH not set for Firecracker executor".to_string(),
                    )
                })?;
                default_firecracker_rootfs_path = env::var("FAAS_DEFAULT_ROOTFS_PATH")
                    .map_err(|_| {
                        BlueprintLibError::MissingEnvVar(
                            "FAAS_DEFAULT_ROOTFS_PATH not set for Firecracker executor".to_string(),
                        )
                    })?
                    .into();

                let fc_executor = FirecrackerExecutor::new(fc_bin, kernel_img)?;
                Arc::new(fc_executor)
            }
            "docker" | _ => {
                if executor_type != "docker" {
                    info!(requested_type = %executor_type, "Defaulting to Docker executor.");
                }
                let builder = DockerBuilder::new()
                    .await
                    .map_err(|e| BlueprintLibError::DockerBuild(e.to_string()))?;
                let docker_executor = DockerExecutor::new(builder.client());
                Arc::new(docker_executor)
            }
        };

        // TODO: Pass default_firecracker_rootfs_path to Orchestrator if needed, or Orchestrator can get it from FaaSContext later.
        let orchestrator = Arc::new(Orchestrator::new(executor));

        Ok(Self {
            config,
            orchestrator,
            executor_type,
            default_firecracker_rootfs_path,
        })
    }
}
