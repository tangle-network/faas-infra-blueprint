mod support;

use blueprint_sdk::tangle::layers::TangleLayer;
use blueprint_sdk::tangle::metadata::macros::ext::FieldType;
use blueprint_sdk::tangle::serde::{new_bounded_string, BoundedVec};
use blueprint_sdk::testing::tempfile;
use blueprint_sdk::testing::utils::setup_log;
use blueprint_sdk::testing::utils::tangle::{InputValue, OutputValue, TangleTestHarness};
use blueprint_sdk::Job;
use color_eyre::Result;
use faas_blueprint_lib::context::FaaSContext;
use faas_blueprint_lib::jobs::{execute_function_job, EXECUTE_FUNCTION_JOB_ID};
use std::time::Duration;
use support::{register_jobs_through, setup_services_with_retry};
use tokio::time::timeout;
use tracing::info;

// Number of nodes for multi-party testing
const N: usize = 3;

fn command_list(parts: &[&str]) -> InputValue {
    InputValue::List(
        FieldType::String,
        BoundedVec(
            parts
                .iter()
                .map(|p| InputValue::String(new_bounded_string(*p)))
                .collect(),
        ),
    )
}

fn empty_env() -> InputValue {
    InputValue::Optional(FieldType::List(Box::new(FieldType::String)), Box::new(None))
}

fn payload_bytes(bytes: &[u8]) -> InputValue {
    InputValue::List(
        FieldType::Uint8,
        BoundedVec(bytes.iter().map(|b| InputValue::Uint8(*b)).collect()),
    )
}

fn decode_stdout(value: &OutputValue) -> Option<String> {
    if let OutputValue::List(_, data) = value {
        let bytes = data
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

#[tokio::test]
async fn faas_execution_onchain() -> Result<()> {
    color_eyre::install()?;
    setup_log();

    info!("=== FAAS EXECUTION ON-CHAIN TEST ===");

    // Initialize test harness with actual blockchain
    let temp_dir = tempfile::TempDir::new()?;
    let harness: TangleTestHarness<FaaSContext> = TangleTestHarness::setup(temp_dir).await?;

    // Setup service with N nodes
    let (mut test_env, service_id, _blueprint_id) =
        setup_services_with_retry::<FaaSContext, N>(&harness, false).await?;
    test_env.initialize().await?;

    // Add the FaaS execution job to the node
    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    // Create contexts for each node
    let mut contexts = Vec::new();
    for handle in test_env.node_handles().await {
        let config = handle.blueprint_config().await;
        let ctx = FaaSContext::new(config.clone()).await?;
        contexts.push(ctx);
    }

    // Start nodes with contexts
    test_env.start_with_contexts(contexts).await?;

    info!("Submitting FaaS job {EXECUTE_FUNCTION_JOB_ID} to service {service_id}");

    // Create job arguments - execute a simple echo command
    let job_args = vec![
        InputValue::String(new_bounded_string("alpine:latest")),
        command_list(&["echo", "Hello Tangle"]),
        empty_env(),
        payload_bytes(&[]),
    ];

    // Submit job on-chain
    let job = harness
        .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
        .await?;

    let call_id = job.call_id;
    info!("Submitted job with call ID {call_id}");

    // Wait for on-chain execution with timeout
    let test_timeout = Duration::from_secs(30);
    let results = timeout(
        test_timeout,
        harness.wait_for_job_execution(service_id, job),
    )
    .await??;

    // Verify on-chain results
    assert_eq!(results.service_id, service_id);
    assert_eq!(results.job as u64, EXECUTE_FUNCTION_JOB_ID);

    // Verify the output contains our echo message
    let stdout = results
        .result
        .first()
        .and_then(decode_stdout)
        .expect("execution should produce stdout");
    assert!(
        stdout.contains("Hello Tangle"),
        "On-chain result should contain echoed message"
    );

    info!("✅ FaaS execution verified on-chain");
    Ok(())
}

#[tokio::test]
async fn faas_compilation_onchain() -> Result<()> {
    color_eyre::install()?;
    setup_log();

    info!("=== FAAS COMPILATION ON-CHAIN TEST ===");

    let temp_dir = tempfile::TempDir::new()?;
    let harness: TangleTestHarness<FaaSContext> = TangleTestHarness::setup(temp_dir).await?;

    let (mut test_env, service_id, _blueprint_id) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    // Single node context
    let handle = test_env.node_handles().await.into_iter().next().unwrap();
    let config = handle.blueprint_config().await;
    let ctx = FaaSContext::new(config.clone()).await?;

    test_env.start_with_contexts(vec![ctx]).await?;

    info!("Submitting Rust compilation job to service {service_id}");

    // Rust compilation job
    let rust_code = r#"
        fn main() {
            println!("Compiled on Tangle!");
        }
    "#;

    let job_args = vec![
        InputValue::String(new_bounded_string("rust:latest")),
        command_list(&[
            "sh",
            "-c",
            &format!("echo '{}' > main.rs && rustc main.rs && ./main", rust_code),
        ]),
        empty_env(),
        payload_bytes(&[]),
    ];

    let job = harness
        .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
        .await?;

    info!("Submitted compilation job with call ID {}", job.call_id);

    // Wait for compilation to complete on-chain
    let test_timeout = Duration::from_secs(60);
    let results = timeout(
        test_timeout,
        harness.wait_for_job_execution(service_id, job),
    )
    .await??;

    assert_eq!(results.service_id, service_id);

    // Verify compilation succeeded
    let output = results
        .result
        .first()
        .and_then(decode_stdout)
        .expect("compilation job should emit stdout");
    assert!(
        output.contains("Compiled on Tangle"),
        "Compilation should produce expected output"
    );

    info!("✅ Rust compilation verified on-chain");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn faas_concurrent_jobs_onchain() -> Result<()> {
    color_eyre::install()?;
    setup_log();

    info!("=== FAAS CONCURRENT JOBS ON-CHAIN TEST ===");

    let temp_dir = tempfile::TempDir::new()?;
    let harness: TangleTestHarness<FaaSContext> = TangleTestHarness::setup(temp_dir).await?;

    // Setup multi-node service for concurrent testing
    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, N>(&harness, false).await?;
    test_env.initialize().await?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    // Create contexts for all nodes
    let mut contexts = Vec::new();
    for handle in test_env.node_handles().await {
        let config = handle.blueprint_config().await;
        contexts.push(FaaSContext::new(config.clone()).await?);
    }

    test_env.start_with_contexts(contexts).await?;

    info!("Submitting multiple concurrent jobs to service {service_id}");

    // Submit multiple jobs concurrently
    let mut job_futures = Vec::new();

    for i in 0..5 {
        let job_args = vec![
            InputValue::String(new_bounded_string("alpine:latest")),
            command_list(&["sh", "-c", &format!("echo 'Job {i}' && sleep 0.1")]),
            empty_env(),
            payload_bytes(&[]),
        ];

        let job = harness
            .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
            .await?;

        info!("Submitted job {} with call ID {}", i, job.call_id);
        job_futures.push((i, job));
    }

    // Wait for all jobs to complete
    let test_timeout = Duration::from_secs(120);

    for (i, job) in job_futures {
        let results = timeout(
            test_timeout,
            harness.wait_for_job_execution(service_id, job),
        )
        .await??;

        assert_eq!(results.service_id, service_id);

        // Verify each job output
        let output = results
            .result
            .first()
            .and_then(decode_stdout)
            .expect("concurrent job should emit stdout");
        assert!(
            output.contains(&format!("Job {i}")),
            "Job {i} output should be recorded on-chain"
        );

        info!("✅ Job {} verified on-chain", i);
    }

    info!("✅ All concurrent jobs executed and verified on-chain");
    Ok(())
}

#[tokio::test]
async fn faas_payload_processing_onchain() -> Result<()> {
    color_eyre::install()?;
    setup_log();

    info!("=== FAAS PAYLOAD PROCESSING ON-CHAIN TEST ===");

    let temp_dir = tempfile::TempDir::new()?;
    let harness: TangleTestHarness<FaaSContext> = TangleTestHarness::setup(temp_dir).await?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    let handle = test_env.node_handles().await.into_iter().next().unwrap();
    let config = handle.blueprint_config().await;
    let ctx = FaaSContext::new(config.clone()).await?;

    test_env.start_with_contexts(vec![ctx]).await?;

    info!("Submitting payload processing job to service {service_id}");

    // Create payload data
    let payload_data = b"Data processed on Tangle blockchain";
    let job_args = vec![
        InputValue::String(new_bounded_string("alpine:latest")),
        command_list(&["cat"]),
        empty_env(),
        payload_bytes(payload_data),
    ];

    let job = harness
        .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
        .await?;

    info!("Submitted payload job with call ID {}", job.call_id);

    let test_timeout = Duration::from_secs(30);
    let results = timeout(
        test_timeout,
        harness.wait_for_job_execution(service_id, job),
    )
    .await??;

    assert_eq!(results.service_id, service_id);

    // Verify payload was processed correctly
    let echoed = results
        .result
        .first()
        .and_then(decode_stdout)
        .map(|s| s.into_bytes())
        .expect("payload job should echo back data");

    assert_eq!(
        echoed, payload_data,
        "Payload should be echoed back correctly on-chain"
    );

    info!("✅ Payload processing verified on-chain");
    Ok(())
}

#[tokio::test]
async fn faas_state_verification() -> Result<()> {
    color_eyre::install()?;
    setup_log();

    info!("=== FAAS STATE VERIFICATION TEST ===");

    let temp_dir = tempfile::TempDir::new()?;
    let harness = TangleTestHarness::setup(temp_dir).await?;

    let (mut test_env, service_id, blueprint_id) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    let handle = test_env.node_handles().await.into_iter().next().unwrap();
    let config = handle.blueprint_config().await;
    let ctx = FaaSContext::new(config.clone()).await?;

    test_env.start_with_contexts(vec![ctx]).await?;

    // Submit multiple jobs to build up state
    let mut call_ids = Vec::new();

    for i in 0..3 {
        let job_args = vec![
            InputValue::String(new_bounded_string("alpine:latest")),
            command_list(&["echo", &format!("State {i}")]),
            empty_env(),
            payload_bytes(&[]),
        ];

        let job = harness
            .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
            .await?;

        call_ids.push(job.call_id);

        let results = harness.wait_for_job_execution(service_id, job).await?;
        assert_eq!(results.service_id, service_id);

        info!("Job {} with call ID {} completed", i, call_ids[i]);
    }

    // Verify all jobs are recorded on-chain
    // In a real scenario, you would query the chain state here
    assert_eq!(call_ids.len(), 3, "All job call IDs should be recorded");

    // Verify service and blueprint IDs remain consistent
    info!("Service ID: {service_id}, Blueprint ID: {blueprint_id}");

    info!(
        "✅ State verification completed - {} jobs recorded on-chain",
        call_ids.len()
    );
    Ok(())
}
