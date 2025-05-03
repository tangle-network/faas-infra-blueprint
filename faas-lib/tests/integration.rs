use blueprint_sdk::{
    self,
    common::{BlueprintEnvironment, ServiceId, TanglePairSigner},
    job::Job,
    primitives::Value,
    tangle::{context::TangleClient, job::TangleLayer, runner::TangleConfig},
    testing::{prelude::TestEnvironmentExt, tangle::TangleTestHarness, JobCallExt},
    Router,
};
use color_eyre::eyre;
use faas_blueprint_lib::jobs::{
    execute_function_job, ExecuteFunctionResult, FaaSExecutionOutput, EXECUTE_FUNCTION_JOB_ID,
};
use faas_executor::common::Executor;
use faas_executor::docktopus::DockerBuilder;
use faas_executor::DockerExecutor;
use faas_orchestrator::Orchestrator;
use parity_scale_codec::Decode;
use std::sync::Arc;
use tempfile::tempdir;

// Helper function to setup the orchestrator
async fn setup_orchestrator() -> eyre::Result<Arc<Orchestrator>> {
    let docker_builder = DockerBuilder::new().await?;
    let docker_client = docker_builder.client();
    let executor: Arc<dyn Executor + Send + Sync> = Arc::new(DockerExecutor::new(docker_client));
    let orchestrator = Arc::new(Orchestrator::new(executor));
    Ok(orchestrator)
}

#[tokio::test]
#[ignore] // Requires Docker daemon and takes time to run
async fn test_faas_job_execution_via_harness() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let temp_dir = tempdir()?;
    let harness = TangleTestHarness::builder(temp_dir.path()).await?;

    // Setup services within harness
    let (mut test_env, service_id, _node_keys) = harness.setup_services::<1>(false).await?;
    let orchestrator = setup_orchestrator().await?;

    test_env.initialize().await?;
    test_env
        .add_job(execute_function_job.layer(TangleLayer))
        .await;
    test_env.start(orchestrator.clone()).await?;

    let image = "alpine:latest".to_string();
    let msg = "Hello via Tangle Harness!";
    let command = vec!["echo".to_string(), msg.to_string()];
    let command_opt: Option<Vec<String>> = Some(command);
    let env_vars_opt: Option<Vec<String>> = None;

    // Encode arguments for TangleArgs3: String, Option<Vec<String>>, Option<Vec<String>>
    let args = (
        Value::String(image),
        Value::Option(
            command_opt
                .map(|cmds| Box::new(Value::List(cmds.into_iter().map(Value::String).collect()))),
        ),
        Value::Option(
            env_vars_opt
                .map(|envs| Box::new(Value::List(envs.into_iter().map(Value::String).collect()))),
        ),
    )
        .encode(); // SCALE encode arguments

    // Submit Job using service_id
    let call = harness
        .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID, args)
        .await?;

    // Wait for Execution
    let result_event = harness.wait_for_job_execution(service_id, &call).await?;

    // Decode the ExecuteFunctionResult from the raw output bytes
    let result_bytes = result_event.output();
    let exec_result = ExecuteFunctionResult::decode(&mut &result_bytes[..])
        .map_err(|e| eyre::eyre!("Failed to decode ExecuteFunctionResult: {}", e))?;

    match exec_result {
        ExecuteFunctionResult::Ok(faas_output) => {
            assert!(faas_output.error.is_none(), "Expected no inner error");
            let expected_stdout = format!("{}\n", msg);
            assert_eq!(
                faas_output.stdout.as_deref().unwrap(),
                expected_stdout.as_str()
            );
            assert!(!faas_output.request_id.is_empty());
            assert!(faas_output.stderr.is_some());
        }
        ExecuteFunctionResult::Err(e) => {
            panic!("Expected Ok result, but got error: {}", e);
        }
    }

    harness.shutdown().await;
    Ok(())
}

#[tokio::test]
#[ignore] // Requires Docker daemon and takes time to run
async fn test_faas_job_execution_failure_via_harness() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let temp_dir = tempdir()?;
    let harness = TangleTestHarness::builder(temp_dir.path()).await?;

    let (mut test_env, service_id, _node_keys) = harness.setup_services::<1>(false).await?;
    let orchestrator = setup_orchestrator().await?;

    test_env.initialize().await?;
    test_env
        .add_job(execute_function_job.layer(TangleLayer))
        .await;
    test_env.start(orchestrator.clone()).await?;

    let image = "alpine:latest".to_string();
    let command = vec!["sh".to_string(), "-c".to_string(), "exit 42".to_string()];
    let command_opt: Option<Vec<String>> = Some(command);
    let env_vars_opt: Option<Vec<String>> = None;

    // Encode arguments
    let args = (
        Value::String(image),
        Value::Option(
            command_opt
                .map(|cmds| Box::new(Value::List(cmds.into_iter().map(Value::String).collect()))),
        ),
        Value::Option(
            env_vars_opt
                .map(|envs| Box::new(Value::List(envs.into_iter().map(Value::String).collect()))),
        ),
    )
        .encode();

    let call = harness
        .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID, args)
        .await?;
    let result_event = harness.wait_for_job_execution(service_id, &call).await?;

    // Decode the ExecuteFunctionResult
    let result_bytes = result_event.output();
    let exec_result = ExecuteFunctionResult::decode(&mut &result_bytes[..])
        .map_err(|e| eyre::eyre!("Failed to decode ExecuteFunctionResult: {}", e))?;

    match exec_result {
        ExecuteFunctionResult::Ok(output) => {
            panic!("Expected Err result, but got Ok: {:?}", output);
        }
        ExecuteFunctionResult::Err(error_string) => {
            assert!(
                error_string
                    .starts_with("FaaS execution failed: Container failed with exit code: 42"),
                "Error message mismatch: Got '{}'",
                error_string
            );
            assert!(
                error_string.contains("(Request ID:"),
                "Error message mismatch: Got '{}'",
                error_string
            );
        }
    }

    harness.shutdown().await;
    Ok(())
}
