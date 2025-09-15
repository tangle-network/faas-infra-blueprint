use async_trait::async_trait; // Import async_trait
use docktopus::bollard::container::{
    AttachContainerOptions, AttachContainerResults, LogOutput, LogsOptions, RemoveContainerOptions,
    WaitContainerOptions,
}; // Import LogsOptions
use docktopus::bollard::errors::Error as BollardError; // Alias bollard error
use docktopus::bollard::Docker;
use docktopus::container::Container; // Use doctopus Container
use faas_common::{
    FaasError, InvocationResult, Result as CommonResult, SandboxConfig, SandboxExecutor,
}; // Use common Result and InvocationResult
use futures::{SinkExt, StreamExt, TryStreamExt}; // Add SinkExt for writing to stdin stream
use std::sync::Arc;
use thiserror::Error;
use tokio::io::AsyncWriteExt; // For write_all
use tracing::{error, info, instrument, warn};
use uuid::Uuid; // Add thiserror

// Re-export dependencies potentially needed by consumers (like orchestrator)
pub use docktopus;
pub use docktopus::bollard;
pub use faas_common as common;

pub mod firecracker;
pub mod executor;
pub mod environment_registry;
pub mod platform;
pub mod performance;

// --- Custom Error Type ---
#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("Container creation failed: {0}")]
    CreationFailed(#[source] BollardError),
    #[error("Container start failed: {0}")]
    StartFailed(#[source] BollardError),
    #[error("Container wait failed: {0}")]
    WaitFailed(#[source] BollardError),
    #[error("Container log retrieval failed: {0}")]
    LogRetrievalFailed(#[source] BollardError),
    #[error("Container removal failed: {0}")]
    RemovalFailed(#[source] BollardError),
    #[error("Docker API error: {0}")]
    DockerApi(#[from] BollardError), // Catch-all for other bollard errors
    #[error("Internal executor error: {0}")]
    Internal(String),
    #[error("Firecracker error: {0}")]
    Firecracker(#[source] firecracker_rs_sdk::Error),
}

// Implement conversion from ExecutorError to the common FaasError
impl From<ExecutorError> for FaasError {
    fn from(err: ExecutorError) -> Self {
        FaasError::Executor(err.to_string()) // Simple conversion for now
    }
}

// Define local Result using the crate's Error type
pub type Result<T> = std::result::Result<T, ExecutorError>;

// Rename InternalContainerConfig and update fields to match SandboxConfig
#[derive(Debug)]
pub struct InternalDockerConfig {
    pub function_id: String,
    pub image: String, // DockerExecutor expects an image
    pub command: Vec<String>,
    pub env_vars: Option<Vec<String>>,
    pub payload: Vec<u8>,
}

// --- DockerExecutor Implementation ---

#[derive(Clone)] // Clone if Arc<Docker> is cloneable
pub struct DockerExecutor {
    docker_client: Arc<Docker>,
}

impl DockerExecutor {
    // Constructor
    pub fn new(docker_client: Arc<Docker>) -> Self {
        Self { docker_client }
    }
}

// Implement SandboxExecutor for DockerExecutor
#[async_trait]
impl SandboxExecutor for DockerExecutor {
    #[instrument(skip(self, config), fields(function_id = %config.function_id, source = %config.source))]
    async fn execute(&self, config: SandboxConfig) -> CommonResult<InvocationResult> {
        // Convert SandboxConfig to the internal config needed by run_container_inner
        let internal_config = InternalDockerConfig {
            function_id: config.function_id,
            image: config.source, // Assume source is the image name for Docker
            command: config.command,
            env_vars: config.env_vars,
            payload: config.payload,
        };
        // Call the actual container running logic
        run_container_inner(self.docker_client.clone(), internal_config)
            .await
            .map_err(FaasError::from) // Convert ExecutorError to FaasError
    }
}

// --- Internal Container Execution Logic ---
// Renamed from run_container to run_container_inner to avoid conflict with trait method
#[instrument(skip(docker_client, config), fields(function_id = %config.function_id, image = %config.image))]
async fn run_container_inner(
    docker_client: Arc<Docker>,
    config: InternalDockerConfig, // Use updated internal config type
) -> Result<InvocationResult> {
    // Returns local ExecutorError Result
    let request_id = Uuid::new_v4().to_string();
    info!(%request_id, function_id=%config.function_id, "Preparing container...");

    // Configure container options, including stdin
    let bollard_config_override = docktopus::bollard::container::Config {
        attach_stdin: Some(true),
        open_stdin: Some(true),
        tty: Some(false), // Ensure TTY is false if using separate streams
        ..Default::default()
    };

    // Use a temporary container name to avoid conflicts if needed
    let temp_container_name = format!("faas-{}-{}", config.function_id, request_id);
    let create_options = Some(docktopus::bollard::container::CreateContainerOptions {
        name: temp_container_name.clone(),
        ..Default::default()
    });

    let container_create_body = docker_client
        .create_container(
            create_options,
            docktopus::bollard::container::Config {
                image: Some(config.image.clone()),
                cmd: Some(config.command.clone()),
                env: config.env_vars.clone(),
                attach_stdin: Some(true),
                open_stdin: Some(true),
                tty: Some(false),
                ..bollard_config_override // Apply other overrides if needed
            },
        )
        .await
        .map_err(ExecutorError::CreationFailed)?;

    let container_id = container_create_body.id;
    info!(%container_id, name=%temp_container_name, "Container created.");

    // Attach streams BEFORE starting
    let attach_options = AttachContainerOptions::<String> {
        stream: Some(true),
        stdin: Some(true),
        stdout: Some(true),
        stderr: Some(true),
        ..Default::default()
    };
    let attach_results = docker_client
        .attach_container(&container_id, Some(attach_options))
        .await
        .map_err(ExecutorError::DockerApi)?;
    let AttachContainerResults {
        mut output,
        mut input,
    } = attach_results;

    // Start the container
    info!(%container_id, "Starting container...");
    docker_client
        .start_container(
            &container_id,
            None::<docktopus::bollard::container::StartContainerOptions<String>>,
        )
        .await
        .map_err(|e| ExecutorError::StartFailed(e))?;

    info!(%container_id, "Container started. Writing payload to stdin...");
    let payload_clone = config.payload.clone();
    let container_id_clone = container_id.clone();
    let stdin_handle = tokio::spawn(async move {
        if let Err(e) = input.write_all(&payload_clone).await {
            error!(error = %e, %container_id_clone, "Failed to write payload to container stdin");
        }
        if let Err(e) = input.shutdown().await {
            error!(error = %e, %container_id_clone, "Failed to shutdown container stdin stream");
        }
        // input is dropped here when task finishes
    });

    info!(%container_id, "Consuming stdout/stderr and waiting for exit...");
    let mut logs_output = Vec::new();
    let container_id_clone = container_id.clone();
    let log_stream_handle = tokio::spawn(async move {
        while let Some(log_entry_res) = output.next().await {
            match log_entry_res {
                Ok(LogOutput::StdOut { message }) | Ok(LogOutput::StdErr { message }) => {
                    logs_output.extend_from_slice(&message);
                }
                Ok(_) => {}
                Err(e) => {
                    error!(error = %e, %container_id_clone, "Error reading container logs stream entry");
                }
            }
        }
        logs_output // Return collected logs
                    // output is dropped here
    });

    // Wait for container to exit
    let wait_options = WaitContainerOptions {
        condition: "not-running",
    };
    let mut wait_stream = docker_client.wait_container(&container_id, Some(wait_options));
    let wait_result = wait_stream.next().await;

    // Ensure stdin task finished (it should have after container exit triggers stream close)
    if let Err(e) = stdin_handle.await {
        error!(error = %e, %container_id, "Stdin write task panicked");
        // Decide if this constitutes a failure of the execution
    }

    // Collect logs from the log stream task
    let logs_output = log_stream_handle.await.unwrap_or_else(|e| {
        error!(error = %e, %container_id, "Log collection task panicked");
        Vec::new()
    });
    let logs_string = String::from_utf8_lossy(&logs_output).to_string();

    // Determine final response and error based on wait_result
    let (response_bytes, error_message) = match wait_result {
        Some(Ok(wait_body)) => {
            let exit_code = wait_body.status_code;
            if exit_code == 0 {
                info!(%container_id, %exit_code, "Container executed successfully");
                (Some(logs_string.clone().into_bytes()), None) // Use logs as response for now
            } else {
                error!(%container_id, %exit_code, "Container exited with non-zero status");
                (
                    None,
                    Some(format!(
                        "Container failed with exit code: {}. Logs: {}",
                        exit_code, logs_string
                    )),
                )
            }
        }
        Some(Err(e)) => {
            error!(%container_id, error=%e, "Container wait() returned Docker error");
            (
                None,
                Some(format!(
                    "Container wait failed: {}. Logs: {}",
                    e, logs_string
                )),
            )
        }
        None => {
            error!(%container_id, "Container wait() stream ended unexpectedly");
            (
                None,
                Some(format!(
                    "Container wait failed: stream ended unexpectedly. Logs: {}",
                    logs_string
                )),
            )
        }
    };

    info!(%container_id, "Removing container...");
    let remove_opts = Some(RemoveContainerOptions {
        force: true,
        ..Default::default()
    });
    if let Err(e) = docker_client
        .remove_container(&container_id, remove_opts)
        .await
    {
        warn!(container_id=%container_id, error = %e, "Failed to remove container");
        // Don't fail the whole execution if cleanup fails, just warn
    }

    Ok(InvocationResult {
        request_id,
        response: response_bytes,
        logs: Some(logs_string),
        error: error_message,
    })
}

// Re-export the unified executor
pub use executor::{Executor, WarmContainer};
