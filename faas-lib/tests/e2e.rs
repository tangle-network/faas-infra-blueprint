use blueprint_sdk::{
    // Group SDK imports
    self,
    common::{BlueprintEnvironment, TanglePairSigner},
    job::Job,
    tangle::{
        // Group tangle imports
        context::TangleClient,
        job::TangleLayer,
        primitives::{InputKey, InputValue, OutputKey, OutputValue},
        runner::TangleConfig,
    },
    testing::{prelude::TestEnvironmentExt, TangleTestHarness},
    Context as BlueprintContext, // Alias if needed, otherwise use `self` path
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
async fn test_faas_job_execution_success() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let dir = tempdir()?;
    let harness = TangleTestHarness::builder(dir.path()).await?;
    // Setup services within harness - gets env, client, signer from harness
    let env = harness.env().await?; // Get env from harness
    let (mut test_env, service_id, node_keys) = harness.setup_services::<1>(false).await?;
    let signer = node_keys.into_iter().next().expect("No signer key").1;
    let client = env.tangle_chain_client(None).await?; // Get client from env

    let orchestrator = setup_orchestrator().await?;

    test_env.initialize().await?;
    // Note: Context for the job handler (Orchestrator) is passed to start()
    test_env
        .add_job(execute_function_job.layer(TangleLayer))
        .await;
    test_env.start(orchestrator.clone()).await?;

    let image = "alpine:latest".to_string();
    let message = "Hello FaaS Lib e2e!";
    let command = vec!["echo".to_string(), message.to_string()];
    let command_opt: Option<Vec<String>> = Some(command);
    let env_vars_opt: Option<Vec<String>> = None;

    let job_inputs = vec![
        (InputKey(0), InputValue::String(image)),
        (
            InputKey(1),
            InputValue::Option(command_opt.map(|cmds| {
                Box::new(InputValue::List(
                    cmds.into_iter().map(InputValue::String).collect(),
                ))
            })),
        ),
        (
            InputKey(2),
            InputValue::Option(env_vars_opt.map(|envs| {
                Box::new(InputValue::List(
                    envs.into_iter().map(InputValue::String).collect(),
                ))
            })),
        ),
    ];

    // Submit job using the service_id from setup_services
    let call = harness.submit_job(service_id, job_inputs).await?;
    let result = harness.wait_for_job_completion(&call).await?;

    assert!(
        !result.is_error(),
        "Expected job to succeed, but it failed with: {:?}",
        result.error_message
    );

    let output_value = result
        .output_map
        .get(&OutputKey(0))
        .expect("Output map missing key 0")
        .clone();
    // Need to deserialize the TangleResult enum first
    let exec_result: ExecuteFunctionResult = output_value
        .try_into()
        .map_err(|e| eyre::eyre!("Failed to deserialize ExecuteFunctionResult: {}", e))?;

    if let ExecuteFunctionResult::Ok(faas_output) = exec_result {
        let expected_stdout = format!("{}\n", message);
        assert!(!faas_output.request_id.is_empty());
        assert_eq!(
            faas_output.stdout.as_deref(),
            Some(expected_stdout.as_str())
        );
        assert_eq!(faas_output.stderr.as_deref(), Some(""));
        assert!(faas_output.error.is_none());
    } else {
        panic!(
            "Expected ExecuteFunctionResult::Ok variant, got Err: {:?}",
            exec_result
        );
    }

    dir.close()?;
    Ok(())
}

#[tokio::test]
async fn test_faas_job_execution_container_error() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let dir = tempdir()?;
    let harness = TangleTestHarness::builder(dir.path()).await?;
    let env = harness.env().await?;
    let (mut test_env, service_id, node_keys) = harness.setup_services::<1>(false).await?;
    let signer = node_keys.into_iter().next().expect("No signer key").1;
    let client = env.tangle_chain_client(None).await?; // Not strictly needed here but good practice
    let orchestrator = setup_orchestrator().await?;

    test_env.initialize().await?;
    test_env
        .add_job(execute_function_job.layer(TangleLayer))
        .await;
    test_env.start(orchestrator.clone()).await?;

    let image = "alpine:latest".to_string();
    let command = vec![
        "sh".to_string(),
        "-c".to_string(),
        "echo 'container error output' >&2; exit 7".to_string(),
    ];
    let command_opt: Option<Vec<String>> = Some(command);
    let env_vars_opt: Option<Vec<String>> = None;

    let job_inputs = vec![
        (InputKey(0), InputValue::String(image)),
        (
            InputKey(1),
            InputValue::Option(command_opt.map(|cmds| {
                Box::new(InputValue::List(
                    cmds.into_iter().map(InputValue::String).collect(),
                ))
            })),
        ),
        (
            InputKey(2),
            InputValue::Option(env_vars_opt.map(|envs| {
                Box::new(InputValue::List(
                    envs.into_iter().map(InputValue::String).collect(),
                ))
            })),
        ),
    ];

    let call = harness.submit_job(service_id, job_inputs).await?;
    let result = harness.wait_for_job_completion(&call).await?;

    assert!(
        !result.is_error(),
        "Expected job completion event, but got framework error: {:?}",
        result.error_message
    );

    let output_value = result
        .output_map
        .get(&OutputKey(0))
        .expect("Output map missing key 0")
        .clone();
    let exec_result: ExecuteFunctionResult = output_value
        .try_into()
        .map_err(|e| eyre::eyre!("Failed to deserialize ExecuteFunctionResult: {}", e))?;

    if let ExecuteFunctionResult::Err(error_string) = exec_result {
        assert!(
            error_string.starts_with("FaaS execution failed: Container failed with exit code: 7"),
            "Error message mismatch: Got '{}'",
            error_string
        );
        assert!(
            error_string.contains("(Request ID:"),
            "Error message mismatch: Got '{}'",
            error_string
        );
        assert!(
            error_string.contains("stderr: Some(\"container error output\\n\")"),
            "Error message mismatch: Got '{}'",
            error_string
        );
    } else {
        panic!(
            "Expected ExecuteFunctionResult::Err variant, got Ok: {:?}",
            exec_result
        );
    }

    dir.close()?;
    Ok(())
}

#[tokio::test]
async fn test_faas_job_orchestrator_error() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let dir = tempdir()?;
    let harness = TangleTestHarness::builder(dir.path()).await?;
    let env = harness.env().await?;
    let (mut test_env, service_id, node_keys) = harness.setup_services::<1>(false).await?;
    let signer = node_keys.into_iter().next().expect("No signer key").1;
    let client = env.tangle_chain_client(None).await?; // Not strictly needed here
    let orchestrator = setup_orchestrator().await?;

    test_env.initialize().await?;
    test_env
        .add_job(execute_function_job.layer(TangleLayer))
        .await;
    test_env.start(orchestrator.clone()).await?;

    let image = "invalid/image/format!".to_string();
    let command = vec!["echo".to_string(), "hello".to_string()];
    let command_opt: Option<Vec<String>> = Some(command);
    let env_vars_opt: Option<Vec<String>> = None;

    let job_inputs = vec![
        (InputKey(0), InputValue::String(image)),
        (
            InputKey(1),
            InputValue::Option(command_opt.map(|cmds| {
                Box::new(InputValue::List(
                    cmds.into_iter().map(InputValue::String).collect(),
                ))
            })),
        ),
        (
            InputKey(2),
            InputValue::Option(env_vars_opt.map(|envs| {
                Box::new(InputValue::List(
                    envs.into_iter().map(InputValue::String).collect(),
                ))
            })),
        ),
    ];

    let call = harness.submit_job(service_id, job_inputs).await?;
    let result = harness.wait_for_job_completion(&call).await?;

    assert!(
        !result.is_error(),
        "Expected job completion event, but got framework error: {:?}",
        result.error_message
    );

    let output_value = result
        .output_map
        .get(&OutputKey(0))
        .expect("Output map missing key 0")
        .clone();
    let exec_result: ExecuteFunctionResult = output_value
        .try_into()
        .map_err(|e| eyre::eyre!("Failed to deserialize ExecuteFunctionResult: {}", e))?;

    if let ExecuteFunctionResult::Err(error_string) = exec_result {
        assert!(
            error_string.starts_with("Orchestrator scheduling failed:"),
            "Error message mismatch: Got '{}'",
            error_string
        );
        assert!(
            error_string.contains("Container creation failed")
                || error_string.contains("invalid reference format"),
            "Error message mismatch: Got '{}'",
            error_string
        );
    } else {
        panic!(
            "Expected ExecuteFunctionResult::Err variant, got Ok: {:?}",
            exec_result
        );
    }

    dir.close()?;
    Ok(())
}
