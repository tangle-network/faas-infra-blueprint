use crate::context::FaaSContext;
use crate::{JobError, EXECUTE_FUNCTION_JOB_ID};
use blueprint_sdk::extract::Context;
use blueprint_sdk::macros::debug_job;
use blueprint_sdk::tangle::extract::{CallId, TangleArg, TangleResult};
use faas_common::{ExecuteFunctionArgs, InvocationResult};
use faas_orchestrator::Orchestrator;
use tracing::{error, info, instrument};

// --- Job Handler Implementation ---

// Takes the Orchestrator via Context and TangleArgs for image, command, and env vars.
#[instrument(skip(ctx), fields(job_id = % EXECUTE_FUNCTION_JOB_ID))]
#[debug_job]
pub async fn execute_function_job(
    Context(ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(args): TangleArg<ExecuteFunctionArgs>,
) -> Result<TangleResult<Vec<u8>>, JobError> {
    info!(image = %args.image, command = ?args.command, "Executing function via Blueprint job");

    let function_id = format!("job_{}", call_id);

    // Call the orchestrator - ASSUMING schedule_execution takes payload
    let invocation_result = match ctx
        .orchestrator
        .schedule_execution(
            function_id,
            args.image,
            args.command,
            args.env_vars,
            args.payload, // This argument needs to be added to schedule_execution
        )
        .await
    {
        Ok(result) => result,
        Err(e) => {
            return Err(JobError::SchedulingFailed(e));
        }
    };

    if let Some(err_msg) = invocation_result.error {
        error!(error_message = %err_msg, logs = ?invocation_result.logs, "Function execution reported error");
        return Err(JobError::FunctionExecutionFailed(InvocationResult {
            request_id: invocation_result.request_id,
            response: invocation_result.response,
            logs: invocation_result.logs,
            error: Some(err_msg),
        }));
    }

    // Refinement task added to Plan.md
    Ok(TangleResult(invocation_result.response.unwrap_or_default()))
}
