use crate::context::FaaSContext;
use crate::JobError;
use blueprint_sdk::extract::Context;
use blueprint_sdk::macros::debug_job;
use blueprint_sdk::tangle::extract::{CallId, TangleArg, TangleResult};
use faas_common::ExecuteFunctionArgs;
use faas_executor::platform::{Mode, Request as PlatformRequest};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, instrument};

// ============================================================================
// STATE-CHANGING JOBS (Require Tangle for state transitions)
// ============================================================================

// --- Execution Jobs ---

pub const EXECUTE_FUNCTION_JOB_ID: u64 = 0;

#[instrument(skip(_ctx), fields(job_id = % EXECUTE_FUNCTION_JOB_ID))]
#[debug_job]
pub async fn execute_function_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(args): TangleArg<ExecuteFunctionArgs>,
) -> Result<TangleResult<Vec<u8>>, JobError> {
    info!(image = %args.image, command = ?args.command, "Executing function");

    let request = PlatformRequest {
        id: format!("job_{}", call_id),
        code: args.command.join(" "),
        mode: Mode::Ephemeral,
        env: args.image,
        timeout: Duration::from_secs(60),
        checkpoint: None,
        branch_from: None,
    };

    let response = _ctx.executor.run(request).await.map_err(|e| {
        JobError::ExecutionFailed(format!("Platform execution failed: {}", e))
    })?;

    if response.exit_code != 0 {
        return Err(JobError::ExecutionFailed(format!(
            "Container exited with code {}",
            response.exit_code
        )));
    }

    Ok(TangleResult(response.stdout))
}

// --- Advanced Execution with Modes ---

pub const EXECUTE_ADVANCED_JOB_ID: u64 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "scale",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct ExecuteAdvancedArgs {
    pub image: String,
    pub command: Vec<String>,
    pub env_vars: Option<Vec<String>>,
    pub payload: Vec<u8>,
    pub mode: String, // "ephemeral", "cached", "checkpointed", "branched", "persistent"
    pub checkpoint_id: Option<String>,
    pub branch_from: Option<String>,
    pub timeout_secs: Option<u64>,
}

#[instrument(skip(_ctx), fields(job_id = % EXECUTE_ADVANCED_JOB_ID))]
#[debug_job]
pub async fn execute_advanced_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(args): TangleArg<ExecuteAdvancedArgs>,
) -> Result<TangleResult<Vec<u8>>, JobError> {
    info!(
        image = %args.image,
        command = ?args.command,
        mode = %args.mode,
        "Executing function with mode"
    );

    let function_id = format!("job_{}", call_id);

    let mode = match args.mode.as_str() {
        "cached" => Mode::Cached,
        "checkpointed" => Mode::Checkpointed,
        "branched" => Mode::Branched,
        "persistent" => Mode::Persistent,
        _ => Mode::Ephemeral,
    };

    let request = PlatformRequest {
        id: function_id,
        code: args.command.join(" "),
        mode,
        env: args.image,
        timeout: Duration::from_secs(args.timeout_secs.unwrap_or(60)),
        checkpoint: args.checkpoint_id,
        branch_from: args.branch_from,
    };

    let response = _ctx.executor.run(request).await.map_err(|e| {
        JobError::ExecutionFailed(format!("Execution failed: {}", e))
    })?;

    Ok(TangleResult(response.stdout))
}

// --- Snapshot Management Jobs ---

pub const CREATE_SNAPSHOT_JOB_ID: u64 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "scale",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct CreateSnapshotArgs {
    pub container_id: String,
    pub name: String,
    pub description: Option<String>,
}

#[instrument(skip(_ctx), fields(job_id = % CREATE_SNAPSHOT_JOB_ID))]
#[debug_job]
pub async fn create_snapshot_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(args): TangleArg<CreateSnapshotArgs>,
) -> Result<TangleResult<String>, JobError> {
    info!(container = %args.container_id, name = %args.name, "Creating snapshot");

    // Use CRIU manager for checkpoint
    let snapshot_id = format!("snap_{}_{}", args.name, call_id);

    // TODO: Actual CRIU checkpoint implementation
    // ctx.criu_manager.checkpoint(args.container_id, snapshot_id)?;

    info!("Created snapshot: {}", snapshot_id);
    Ok(TangleResult(snapshot_id))
}

pub const RESTORE_SNAPSHOT_JOB_ID: u64 = 3;

#[instrument(skip(_ctx), fields(job_id = % RESTORE_SNAPSHOT_JOB_ID))]
#[debug_job]
pub async fn restore_snapshot_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(snapshot_id): TangleArg<String>,
) -> Result<TangleResult<String>, JobError> {
    info!("Restoring snapshot: {}", snapshot_id);

    let container_id = format!("restored_{}_{}", snapshot_id, call_id);

    // TODO: Actual CRIU restore implementation
    // ctx.criu_manager.restore(snapshot_id, container_id)?;

    info!("Restored container: {}", container_id);
    Ok(TangleResult(container_id))
}

// --- Branching Jobs ---

pub const CREATE_BRANCH_JOB_ID: u64 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "scale",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct CreateBranchArgs {
    pub parent_snapshot_id: String,
    pub branch_name: String,
}

#[instrument(skip(_ctx), fields(job_id = % CREATE_BRANCH_JOB_ID))]
#[debug_job]
pub async fn create_branch_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(args): TangleArg<CreateBranchArgs>,
) -> Result<TangleResult<String>, JobError> {
    info!(parent = %args.parent_snapshot_id, name = %args.branch_name, "Creating branch");

    let branch_id = format!("branch_{}_{}", args.branch_name, call_id);

    // TODO: Use ForkManager for COW branching
    // ctx.fork_manager.create_branch(args.parent_snapshot_id, branch_id)?;

    info!("Created branch: {}", branch_id);
    Ok(TangleResult(branch_id))
}

pub const MERGE_BRANCHES_JOB_ID: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "scale",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct MergeBranchesArgs {
    pub branch_ids: Vec<String>,
    pub merge_strategy: String, // "union", "intersection", "latest"
}

#[instrument(skip(_ctx), fields(job_id = % MERGE_BRANCHES_JOB_ID))]
#[debug_job]
pub async fn merge_branches_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(args): TangleArg<MergeBranchesArgs>,
) -> Result<TangleResult<String>, JobError> {
    info!("Merging branches: {:?}", args.branch_ids);

    let merged_id = format!("merged_{}", call_id);

    // TODO: Implement branch merging logic

    info!("Merged into: {}", merged_id);
    Ok(TangleResult(merged_id))
}

// --- Instance Management Jobs ---

pub const START_INSTANCE_JOB_ID: u64 = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "scale",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct StartInstanceArgs {
    pub snapshot_id: Option<String>,
    pub image: Option<String>,
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub enable_ssh: bool,
}

#[instrument(skip(_ctx), fields(job_id = % START_INSTANCE_JOB_ID))]
#[debug_job]
pub async fn start_instance_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(args): TangleArg<StartInstanceArgs>,
) -> Result<TangleResult<String>, JobError> {
    info!("Starting persistent instance");

    let instance_id = format!("inst_{}", call_id);

    // TODO: Create long-running container with SSH if enabled
    // - Use snapshot_id if provided, otherwise use image
    // - Configure resources
    // - Setup SSH server if enable_ssh

    info!("Started instance: {}", instance_id);
    Ok(TangleResult(instance_id))
}

pub const STOP_INSTANCE_JOB_ID: u64 = 7;

#[instrument(skip(_ctx), fields(job_id = % STOP_INSTANCE_JOB_ID))]
#[debug_job]
pub async fn stop_instance_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(instance_id): TangleArg<String>,
) -> Result<TangleResult<bool>, JobError> {
    info!("Stopping instance: {}", instance_id);

    // TODO: Stop the container/VM

    info!("Stopped instance: {}", instance_id);
    Ok(TangleResult(true))
}

pub const PAUSE_INSTANCE_JOB_ID: u64 = 8;

#[instrument(skip(_ctx), fields(job_id = % PAUSE_INSTANCE_JOB_ID))]
#[debug_job]
pub async fn pause_instance_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(instance_id): TangleArg<String>,
) -> Result<TangleResult<String>, JobError> {
    info!("Pausing instance: {}", instance_id);

    // Create checkpoint and pause
    let checkpoint_id = format!("pause_{}_{}", instance_id, call_id);

    // TODO: CRIU checkpoint + pause container

    info!("Paused with checkpoint: {}", checkpoint_id);
    Ok(TangleResult(checkpoint_id))
}

pub const RESUME_INSTANCE_JOB_ID: u64 = 9;

#[instrument(skip(_ctx), fields(job_id = % RESUME_INSTANCE_JOB_ID))]
#[debug_job]
pub async fn resume_instance_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(checkpoint_id): TangleArg<String>,
) -> Result<TangleResult<String>, JobError> {
    info!("Resuming from checkpoint: {}", checkpoint_id);

    // TODO: CRIU restore from checkpoint
    let instance_id = format!("resumed_{}", call_id);

    info!("Resumed instance: {}", instance_id);
    Ok(TangleResult(instance_id))
}

// --- Port Management Jobs ---

pub const EXPOSE_PORT_JOB_ID: u64 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "scale",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct ExposePortArgs {
    pub instance_id: String,
    pub internal_port: u16,
    pub protocol: String, // "http", "https", "tcp"
    pub subdomain: Option<String>,
}

#[instrument(skip(_ctx), fields(job_id = % EXPOSE_PORT_JOB_ID))]
#[debug_job]
pub async fn expose_port_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(args): TangleArg<ExposePortArgs>,
) -> Result<TangleResult<String>, JobError> {
    info!(instance = %args.instance_id, port = args.internal_port, "Exposing port");

    // TODO: Configure reverse proxy or port mapping
    let public_url = format!("https://{}.faas.local:{}",
        args.subdomain.unwrap_or_else(|| args.instance_id.clone()),
        args.internal_port
    );

    info!("Exposed at: {}", public_url);
    Ok(TangleResult(public_url))
}

// --- File Operation Jobs ---

pub const UPLOAD_FILES_JOB_ID: u64 = 11;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "scale",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct UploadFilesArgs {
    pub instance_id: String,
    pub target_path: String,
    pub files_data: Vec<u8>, // Tar archive or similar
}

#[instrument(skip(_ctx), fields(job_id = % UPLOAD_FILES_JOB_ID))]
#[debug_job]
pub async fn upload_files_job(
    Context(_ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(args): TangleArg<UploadFilesArgs>,
) -> Result<TangleResult<u64>, JobError> {
    info!(instance = %args.instance_id, path = %args.target_path, "Uploading files");

    let bytes_uploaded = args.files_data.len() as u64;

    // TODO: Extract and copy files to container

    info!("Uploaded {} bytes", bytes_uploaded);
    Ok(TangleResult(bytes_uploaded))
}