use blueprint_sdk::tangle::layers::TangleLayer;
use blueprint_sdk::testing::tempfile;
use blueprint_sdk::testing::utils::setup_log;
use blueprint_sdk::testing::utils::tangle::{InputValue, TangleTestHarness};
use blueprint_sdk::Job;
use color_eyre::Result;
use faas_blueprint_lib::jobs::execute_function_job;

#[tokio::test]
async fn test_minimal() -> Result<()> {
    color_eyre::install().ok();
    setup_log();

    // Initialize test harness - let it default to () context
    let temp_dir = tempfile::TempDir::new()?;
    let harness: TangleTestHarness<()> = TangleTestHarness::setup(temp_dir).await?;

    // Setup service
    let (mut test_env, service_id, _blueprint_id) = harness.setup_services::<1>(false).await?;
    test_env.initialize().await?;

    // Add job and start with unit context
    test_env.add_job(execute_function_job.layer(TangleLayer)).await;
    test_env.start(()).await?;

    // Submit job
    let job = harness
        .submit_job(service_id, 0, vec![
            InputValue::String(blueprint_sdk::tangle::serde::new_bounded_string("alpine:latest")),
            InputValue::List(blueprint_sdk::tangle::metadata::macros::ext::FieldType::String, blueprint_sdk::tangle::serde::BoundedVec(vec![
                InputValue::String(blueprint_sdk::tangle::serde::new_bounded_string("echo")),
                InputValue::String(blueprint_sdk::tangle::serde::new_bounded_string("test")),
            ])),
            InputValue::List(blueprint_sdk::tangle::metadata::macros::ext::FieldType::String, blueprint_sdk::tangle::serde::BoundedVec(vec![])),
            InputValue::List(blueprint_sdk::tangle::metadata::macros::ext::FieldType::Uint8, blueprint_sdk::tangle::serde::BoundedVec(vec![])),
        ])
        .await?;
    
    let results = harness.wait_for_job_execution(service_id, job).await?;
    assert_eq!(results.service_id, service_id);
    Ok(())
}
