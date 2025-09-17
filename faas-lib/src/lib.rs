use faas_common::InvocationResult;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod context;
pub mod jobs;

pub const EXECUTE_FUNCTION_JOB_ID: u64 = 0;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[cfg_attr(
    feature = "scale",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct FaaSExecutionOutput {
    pub request_id: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub error: Option<String>,
}

impl From<InvocationResult> for FaaSExecutionOutput {
    fn from(result: InvocationResult) -> Self {
        // Attempt to decode stdout/stderr if they are UTF-8
        let stdout = result
            .response
            .and_then(|bytes| String::from_utf8(bytes).ok());

        // Note: The current InvocationResult bundles all logs (stdout+stderr) into `logs`.
        // We'll put the combined logs into `stderr` field for now, and keep `stdout` based on `response`.
        // Ideally, InvocationResult would separate stdout and stderr.
        let stderr = result.logs;

        Self {
            request_id: result.request_id,
            stdout,
            stderr,
            error: result.error,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "scale",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub enum ExecuteFunctionResult {
    Ok(FaaSExecutionOutput),
    Err(String),
}

impl ExecuteFunctionResult {
    pub fn ok(output: FaaSExecutionOutput) -> Self {
        Self::Ok(output)
    }

    pub fn err(message: String) -> Self {
        Self::Err(message)
    }
}

impl From<FaaSExecutionOutput> for ExecuteFunctionResult {
    fn from(output: FaaSExecutionOutput) -> Self {
        Self::Ok(output)
    }
}

#[derive(Error, Debug)]
pub enum JobError {
    #[error("Orchestrator scheduling failed: {0}")]
    SchedulingFailed(#[from] faas_orchestrator::Error),
    #[error("FaaS execution failed: {0}")]
    FunctionExecutionFailed(InvocationResult),
    #[error("Invalid job input: {0}")]
    InvalidInput(String),
}
