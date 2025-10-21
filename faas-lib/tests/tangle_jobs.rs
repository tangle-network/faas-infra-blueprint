mod support;

use blueprint_sdk::tangle::metadata::macros::ext::FieldType;
use blueprint_sdk::tangle::serde::{from_field, new_bounded_string, BoundedVec};
use blueprint_sdk::testing::tempfile;
use blueprint_sdk::testing::utils::setup_log;
use blueprint_sdk::testing::utils::tangle::{
    multi_node::MultiNodeTestEnv, runner::MockHeartbeatConsumer, InputValue, OutputValue,
    TangleTestHarness,
};
use color_eyre::eyre::eyre;
use color_eyre::Result;
use faas_blueprint_lib::context::FaaSContext;
use faas_blueprint_lib::jobs::{
    CreateBranchArgs, CreateSnapshotArgs, CREATE_BRANCH_JOB_ID, CREATE_SNAPSHOT_JOB_ID,
    EXECUTE_ADVANCED_JOB_ID, EXECUTE_FUNCTION_JOB_ID, RESTORE_SNAPSHOT_JOB_ID,
};
use std::time::Duration;
use support::{register_jobs_through, setup_services_with_retry};
use tokio::time::timeout;
use tracing::info;

trait ToEyreResult<T> {
    fn to_eyre(self) -> Result<T>;
}

impl<T, E: std::fmt::Display> ToEyreResult<T> for std::result::Result<T, E> {
    fn to_eyre(self) -> Result<T> {
        self.map_err(|e| eyre!("{}", e))
    }
}

fn execute_args(image: &str, command: &[&str]) -> Vec<InputValue> {
    vec![
        InputValue::String(new_bounded_string(image)),
        InputValue::List(
            FieldType::String,
            BoundedVec(
                command
                    .iter()
                    .map(|s| InputValue::String(new_bounded_string(*s)))
                    .collect(),
            ),
        ),
        InputValue::Optional(FieldType::List(Box::new(FieldType::String)), Box::new(None)),
        InputValue::List(FieldType::Uint8, BoundedVec(vec![])),
    ]
}

fn advanced_args(
    image: &str,
    command: &[&str],
    mode: &str,
    timeout_secs: Option<u64>,
) -> Vec<InputValue> {
    vec![
        InputValue::String(new_bounded_string(image)),
        InputValue::List(
            FieldType::String,
            BoundedVec(
                command
                    .iter()
                    .map(|s| InputValue::String(new_bounded_string(*s)))
                    .collect(),
            ),
        ),
        InputValue::Optional(FieldType::List(Box::new(FieldType::String)), Box::new(None)),
        InputValue::List(FieldType::Uint8, BoundedVec(vec![])),
        InputValue::String(new_bounded_string(mode)),
        InputValue::Optional(FieldType::String, Box::new(None)),
        InputValue::Optional(FieldType::String, Box::new(None)),
        InputValue::Optional(
            FieldType::Uint64,
            Box::new(timeout_secs.map(InputValue::Uint64)),
        ),
    ]
}

fn expect_string(output: Option<&OutputValue>) -> Result<String> {
    match output {
        Some(value) => from_field(value.clone()).map_err(|e| eyre!(e)),
        None => Err(eyre!("expected string output, got None")),
    }
}

fn decode_stdout(value: &OutputValue) -> Option<String> {
    if let OutputValue::List(_, list) = value {
        let bytes = list
            .0
            .iter()
            .filter_map(|field| {
                if let OutputValue::Uint8(byte) = field {
                    Some(*byte)
                } else {
                    None
                }
            })
            .collect::<Vec<u8>>();
        Some(String::from_utf8_lossy(&bytes).to_string())
    } else {
        None
    }
}

async fn start_with_faas_contexts(
    env: &mut MultiNodeTestEnv<FaaSContext, MockHeartbeatConsumer>,
) -> Result<()> {
    let mut contexts = Vec::new();
    for handle in env.node_handles().await {
        let config = handle.blueprint_config().await;
        contexts.push(FaaSContext::new(config).await.to_eyre()?);
    }
    env.start_with_contexts(contexts).await.to_eyre()
}

#[tokio::test]
async fn test_execute_function_job() -> Result<()> {
    let _ = color_eyre::install();
    setup_log();

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness: TangleTestHarness<FaaSContext> =
        TangleTestHarness::setup(temp_dir).await.to_eyre()?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    start_with_faas_contexts(&mut test_env).await?;

    let job_args = execute_args("alpine:latest", &["echo", "hello"]);
    let job = harness
        .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
        .await
        .to_eyre()?;
    info!(
        service_id,
        call_id = job.call_id,
        "Submitted execute_function job"
    );

    let results = timeout(
        Duration::from_secs(60),
        harness.wait_for_job_execution(service_id, job),
    )
    .await
    .to_eyre()??;

    let output = results
        .result
        .first()
        .and_then(decode_stdout)
        .ok_or_else(|| eyre!("expected byte list output from execution"))?;
    assert!(
        output.contains("hello"),
        "expected container output to include greeting, got {output}"
    );

    Ok(())
}

#[tokio::test]
async fn test_execute_advanced_job() -> Result<()> {
    let _ = color_eyre::install();
    setup_log();

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness: TangleTestHarness<FaaSContext> =
        TangleTestHarness::setup(temp_dir).await.to_eyre()?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_ADVANCED_JOB_ID).await;

    start_with_faas_contexts(&mut test_env).await?;

    let job_args = advanced_args("alpine:latest", &["echo", "cached run"], "cached", Some(30));

    let job = harness
        .submit_job(service_id, EXECUTE_ADVANCED_JOB_ID as u8, job_args)
        .await
        .to_eyre()?;
    info!(
        service_id,
        call_id = job.call_id,
        "Submitted execute_advanced job"
    );

    let results = timeout(
        Duration::from_secs(60),
        harness.wait_for_job_execution(service_id, job),
    )
    .await
    .to_eyre()??;

    let output = results
        .result
        .first()
        .and_then(decode_stdout)
        .ok_or_else(|| eyre!("expected byte list output from advanced execution"))?;
    assert!(
        output.contains("cached run"),
        "expected cached execution output, got {output}"
    );

    Ok(())
}

#[tokio::test]
async fn test_snapshot_lifecycle() -> Result<()> {
    let _ = color_eyre::install();
    setup_log();

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness: TangleTestHarness<FaaSContext> =
        TangleTestHarness::setup(temp_dir).await.to_eyre()?;
    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await.to_eyre()?;
    register_jobs_through(&mut test_env, RESTORE_SNAPSHOT_JOB_ID).await;

    start_with_faas_contexts(&mut test_env).await?;

    let create_inputs = vec![
        InputValue::String(new_bounded_string("container_123")),
        InputValue::String(new_bounded_string("test_snapshot")),
        InputValue::Optional(
            FieldType::String,
            Box::new(Some(InputValue::String(new_bounded_string(
                "snapshot for testing",
            )))),
        ),
    ];

    let create_job = harness
        .submit_job(service_id, CREATE_SNAPSHOT_JOB_ID as u8, create_inputs)
        .await
        .to_eyre()?;
    info!(
        service_id,
        call_id = create_job.call_id,
        "Submitted create_snapshot job"
    );

    let create_results = timeout(
        Duration::from_secs(60),
        harness.wait_for_job_execution(service_id, create_job),
    )
    .await
    .to_eyre()??;

    let snapshot_id = expect_string(create_results.result.first())?;

    let restore_input = InputValue::String(new_bounded_string(&snapshot_id));
    let restore_job = harness
        .submit_job(
            service_id,
            RESTORE_SNAPSHOT_JOB_ID as u8,
            vec![restore_input],
        )
        .await
        .to_eyre()?;
    info!(
        service_id,
        call_id = restore_job.call_id,
        "Submitted restore_snapshot job"
    );

    let restore_results = timeout(
        Duration::from_secs(60),
        harness.wait_for_job_execution(service_id, restore_job),
    )
    .await
    .to_eyre()??;

    let container_id = expect_string(restore_results.result.first())?;
    assert!(
        container_id.contains("restored"),
        "restored container id should be namespaced, got {container_id}"
    );

    Ok(())
}

#[tokio::test]
async fn test_branch_creation() -> Result<()> {
    let _ = color_eyre::install();
    setup_log();

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness: TangleTestHarness<FaaSContext> =
        TangleTestHarness::setup(temp_dir).await.to_eyre()?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await.to_eyre()?;
    register_jobs_through(&mut test_env, CREATE_BRANCH_JOB_ID).await;

    start_with_faas_contexts(&mut test_env).await?;

    let branch_inputs = vec![
        InputValue::String(new_bounded_string("snap_parent_123")),
        InputValue::String(new_bounded_string("feature_branch")),
    ];

    let job = harness
        .submit_job(service_id, CREATE_BRANCH_JOB_ID as u8, branch_inputs)
        .await
        .to_eyre()?;
    info!(
        service_id,
        call_id = job.call_id,
        "Submitted create_branch job"
    );

    let results = timeout(
        Duration::from_secs(60),
        harness.wait_for_job_execution(service_id, job),
    )
    .await
    .to_eyre()??;

    let branch_id = expect_string(results.result.first())?;
    assert!(
        branch_id.contains("branch_feature_branch"),
        "branch identifier should include name, got {branch_id}"
    );

    Ok(())
}
