use blueprint_sdk::testing::{InputValue, OutputValue, TangleTestHarness};
use faas_lib::context::FaaSContext;
use faas_lib::jobs::*;
use std::time::Duration;
use tokio::time::timeout;

/// Test basic function execution job
#[tokio::test]
async fn test_execute_function_job() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::TempDir::new()?;
    let harness = TangleTestHarness::setup(temp_dir).await?;

    let (mut test_env, service_id, _) = harness.setup_services::<1>(false).await?;
    test_env.initialize().await?;

    // Add the execute function job
    test_env
        .add_job(execute_function_job.layer(blueprint_sdk::tangle::layer::TangleLayer))
        .await;

    test_env.start(()).await?;

    // Submit job with basic execution args
    let job_args = vec![
        InputValue::String("alpine:latest".into()), // image
        InputValue::List(vec![
            InputValue::String("echo".into()),
            InputValue::String("hello".into()),
        ]), // command
        InputValue::None, // env_vars
        InputValue::Bytes(vec![]), // payload
    ];

    let test_timeout = Duration::from_secs(30);
    let job = harness
        .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID, job_args)
        .await?;

    let results = timeout(test_timeout, harness.wait_for_job_execution(service_id, job)).await??;

    // Verify job executed successfully
    assert_eq!(results.service_id, service_id);
    assert!(results.call_id > 0);

    // The output should contain "hello"
    if let Some(OutputValue::Bytes(output)) = results.result.first() {
        let output_str = String::from_utf8_lossy(output);
        assert!(output_str.contains("hello"));
    } else {
        panic!("Expected bytes output");
    }

    Ok(())
}

/// Test advanced execution with modes
#[tokio::test]
async fn test_execute_advanced_job() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::TempDir::new()?;
    let harness = TangleTestHarness::setup(temp_dir).await?;

    let (mut test_env, service_id, _) = harness.setup_services::<1>(false).await?;
    test_env.initialize().await?;

    test_env
        .add_job(execute_advanced_job.layer(blueprint_sdk::tangle::layer::TangleLayer))
        .await;

    test_env.start(()).await?;

    // Test cached mode execution
    let job_args = vec![
        InputValue::String("alpine:latest".into()), // image
        InputValue::List(vec![
            InputValue::String("echo".into()),
            InputValue::String("cached test".into()),
        ]), // command
        InputValue::None, // env_vars
        InputValue::Bytes(vec![]), // payload
        InputValue::String("cached".into()), // mode
        InputValue::None, // checkpoint_id
        InputValue::None, // branch_from
        InputValue::Uint64(30), // timeout_secs
    ];

    let test_timeout = Duration::from_secs(30);
    let job = harness
        .submit_job(service_id, EXECUTE_ADVANCED_JOB_ID, job_args)
        .await?;

    let results = timeout(test_timeout, harness.wait_for_job_execution(service_id, job)).await??;

    assert_eq!(results.service_id, service_id);

    Ok(())
}

/// Test snapshot creation and restoration
#[tokio::test]
async fn test_snapshot_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::TempDir::new()?;
    let harness = TangleTestHarness::setup(temp_dir).await?;

    let (mut test_env, service_id, _) = harness.setup_services::<1>(false).await?;
    test_env.initialize().await?;

    // Add snapshot jobs
    test_env
        .add_job(create_snapshot_job.layer(blueprint_sdk::tangle::layer::TangleLayer))
        .await;
    test_env
        .add_job(restore_snapshot_job.layer(blueprint_sdk::tangle::layer::TangleLayer))
        .await;

    test_env.start(()).await?;

    // Create a snapshot
    let create_args = vec![
        InputValue::String("container_123".into()), // container_id
        InputValue::String("test_snapshot".into()), // name
        InputValue::String("Test snapshot for testing".into()), // description
    ];

    let test_timeout = Duration::from_secs(30);
    let create_job = harness
        .submit_job(service_id, CREATE_SNAPSHOT_JOB_ID, create_args)
        .await?;

    let create_results = timeout(
        test_timeout,
        harness.wait_for_job_execution(service_id, create_job),
    )
    .await??;

    // Extract snapshot ID
    let snapshot_id = if let Some(OutputValue::String(id)) = create_results.result.first() {
        id.clone()
    } else {
        panic!("Expected string snapshot ID");
    };

    // Restore from snapshot
    let restore_args = vec![InputValue::String(snapshot_id.clone())];

    let restore_job = harness
        .submit_job(service_id, RESTORE_SNAPSHOT_JOB_ID, restore_args)
        .await?;

    let restore_results = timeout(
        test_timeout,
        harness.wait_for_job_execution(service_id, restore_job),
    )
    .await??;

    // Verify restoration created a new container
    if let Some(OutputValue::String(container_id)) = restore_results.result.first() {
        assert!(container_id.contains("restored"));
    } else {
        panic!("Expected container ID from restore");
    }

    Ok(())
}

/// Test branch creation
#[tokio::test]
async fn test_branch_creation() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::TempDir::new()?;
    let harness = TangleTestHarness::setup(temp_dir).await?;

    let (mut test_env, service_id, _) = harness.setup_services::<1>(false).await?;
    test_env.initialize().await?;

    test_env
        .add_job(create_branch_job.layer(blueprint_sdk::tangle::layer::TangleLayer))
        .await;

    test_env.start(()).await?;

    let branch_args = vec![
        InputValue::String("snap_parent_123".into()), // parent_snapshot_id
        InputValue::String("feature_branch".into()),  // branch_name
    ];

    let test_timeout = Duration::from_secs(30);
    let job = harness
        .submit_job(service_id, CREATE_BRANCH_JOB_ID, branch_args)
        .await?;

    let results = timeout(test_timeout, harness.wait_for_job_execution(service_id, job)).await??;

    // Verify branch was created
    if let Some(OutputValue::String(branch_id)) = results.result.first() {
        assert!(branch_id.contains("branch_feature_branch"));
    } else {
        panic!("Expected branch ID");
    }

    Ok(())
}

/// Test instance management
#[tokio::test]
async fn test_instance_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::TempDir::new()?;
    let harness = TangleTestHarness::setup(temp_dir).await?;

    let (mut test_env, service_id, _) = harness.setup_services::<1>(false).await?;
    test_env.initialize().await?;

    // Add instance jobs
    test_env
        .add_job(start_instance_job.layer(blueprint_sdk::tangle::layer::TangleLayer))
        .await;
    test_env
        .add_job(pause_instance_job.layer(blueprint_sdk::tangle::layer::TangleLayer))
        .await;
    test_env
        .add_job(resume_instance_job.layer(blueprint_sdk::tangle::layer::TangleLayer))
        .await;
    test_env
        .add_job(stop_instance_job.layer(blueprint_sdk::tangle::layer::TangleLayer))
        .await;

    test_env.start(()).await?;

    // Start an instance
    let start_args = vec![
        InputValue::None,                            // snapshot_id
        InputValue::String("ubuntu:latest".into()),  // image
        InputValue::Uint32(2),                       // cpu_cores
        InputValue::Uint32(4096),                    // memory_mb
        InputValue::Uint32(20),                      // disk_gb
        InputValue::Bool(true),                      // enable_ssh
    ];

    let test_timeout = Duration::from_secs(30);
    let start_job = harness
        .submit_job(service_id, START_INSTANCE_JOB_ID, start_args)
        .await?;

    let start_results = timeout(
        test_timeout,
        harness.wait_for_job_execution(service_id, start_job),
    )
    .await??;

    let instance_id = if let Some(OutputValue::String(id)) = start_results.result.first() {
        id.clone()
    } else {
        panic!("Expected instance ID");
    };

    // Pause the instance
    let pause_args = vec![InputValue::String(instance_id.clone())];
    let pause_job = harness
        .submit_job(service_id, PAUSE_INSTANCE_JOB_ID, pause_args)
        .await?;

    let pause_results = timeout(
        test_timeout,
        harness.wait_for_job_execution(service_id, pause_job),
    )
    .await??;

    let checkpoint_id = if let Some(OutputValue::String(id)) = pause_results.result.first() {
        id.clone()
    } else {
        panic!("Expected checkpoint ID from pause");
    };

    // Resume the instance
    let resume_args = vec![InputValue::String(checkpoint_id)];
    let resume_job = harness
        .submit_job(service_id, RESUME_INSTANCE_JOB_ID, resume_args)
        .await?;

    let resume_results = timeout(
        test_timeout,
        harness.wait_for_job_execution(service_id, resume_job),
    )
    .await??;

    assert!(resume_results.result.first().is_some());

    // Stop the instance
    let stop_args = vec![InputValue::String(instance_id)];
    let stop_job = harness
        .submit_job(service_id, STOP_INSTANCE_JOB_ID, stop_args)
        .await?;

    let stop_results = timeout(
        test_timeout,
        harness.wait_for_job_execution(service_id, stop_job),
    )
    .await??;

    if let Some(OutputValue::Bool(stopped)) = stop_results.result.first() {
        assert!(stopped);
    } else {
        panic!("Expected boolean stop result");
    }

    Ok(())
}

/// Test port exposure
#[tokio::test]
async fn test_expose_port() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::TempDir::new()?;
    let harness = TangleTestHarness::setup(temp_dir).await?;

    let (mut test_env, service_id, _) = harness.setup_services::<1>(false).await?;
    test_env.initialize().await?;

    test_env
        .add_job(expose_port_job.layer(blueprint_sdk::tangle::layer::TangleLayer))
        .await;

    test_env.start(()).await?;

    let port_args = vec![
        InputValue::String("inst_123".into()),       // instance_id
        InputValue::Uint16(8080),                    // internal_port
        InputValue::String("http".into()),           // protocol
        InputValue::String("myapp".into()),          // subdomain
    ];

    let test_timeout = Duration::from_secs(30);
    let job = harness
        .submit_job(service_id, EXPOSE_PORT_JOB_ID, port_args)
        .await?;

    let results = timeout(test_timeout, harness.wait_for_job_execution(service_id, job)).await??;

    // Verify URL was generated
    if let Some(OutputValue::String(url)) = results.result.first() {
        assert!(url.contains("myapp.faas.local"));
        assert!(url.contains("8080"));
    } else {
        panic!("Expected URL string");
    }

    Ok(())
}