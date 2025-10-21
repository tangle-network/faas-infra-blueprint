mod support;

use blueprint_sdk::tangle::layers::TangleLayer;
use blueprint_sdk::tangle::metadata::macros::ext::FieldType;
use blueprint_sdk::tangle::serde::{new_bounded_string, BoundedVec};
use blueprint_sdk::testing::tempfile;
use blueprint_sdk::testing::utils::setup_log;
use blueprint_sdk::testing::utils::tangle::{InputValue, TangleTestHarness};
use blueprint_sdk::Job;
use color_eyre::Result;
use faas_blueprint_lib::context::FaaSContext;
use faas_blueprint_lib::jobs::execute_function_job;
use support::{register_jobs_through, setup_services_with_retry};

#[tokio::test]
async fn test_minimal() -> Result<()> {
    color_eyre::install().ok();
    setup_log();

    // Initialize test harness - let it default to () context
    let temp_dir = tempfile::TempDir::new()?;
    let harness: TangleTestHarness<FaaSContext> = TangleTestHarness::setup(temp_dir).await?;

    // Setup service
    let (mut test_env, service_id, _blueprint_id) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;
    let mut contexts = Vec::new();
    for handle in test_env.node_handles().await {
        let config = handle.blueprint_config().await;
        contexts.push(FaaSContext::new(config).await?);
    }
    test_env.start_with_contexts(contexts).await?;

    // Submit job
    let job = harness
        .submit_job(
            service_id,
            0,
            vec![
                InputValue::String(new_bounded_string("alpine:latest")),
                InputValue::List(
                    FieldType::String,
                    BoundedVec(vec![
                        InputValue::String(new_bounded_string("echo")),
                        InputValue::String(new_bounded_string("test")),
                    ]),
                ),
                InputValue::Optional(FieldType::List(Box::new(FieldType::String)), Box::new(None)),
                InputValue::List(FieldType::Uint8, BoundedVec(vec![])),
            ],
        )
        .await?;

    let results = harness.wait_for_job_execution(service_id, job).await?;
    assert_eq!(results.service_id, service_id);
    Ok(())
}
