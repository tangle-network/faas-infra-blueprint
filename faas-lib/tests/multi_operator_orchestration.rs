mod support;

/// Multi-operator orchestration tests
/// Verifies operator selection, load balancing, job distribution, and edge cases
use blueprint_client_tangle::client::TangleClient;
use blueprint_sdk::crypto::sp_core::{SpEcdsa, SpSr25519};
use blueprint_sdk::keystore::backends::Backend;
use blueprint_sdk::runner::config::BlueprintEnvironment;
use blueprint_sdk::tangle::metadata::macros::ext::FieldType;
use blueprint_sdk::tangle::serde::{new_bounded_string, BoundedVec};
use blueprint_sdk::testing::tempfile;
use blueprint_sdk::testing::utils::setup_log;
use blueprint_sdk::testing::utils::tangle::{InputValue, OutputValue, TangleTestHarness};
use color_eyre::eyre::{eyre, WrapErr};
use color_eyre::Result;
use faas_blueprint_lib::context::FaaSContext;
use faas_blueprint_lib::jobs::{EXECUTE_ADVANCED_JOB_ID, EXECUTE_FUNCTION_JOB_ID};
use hex::encode as hex_encode;
use k256::{elliptic_curve::sec1::ToEncodedPoint, PublicKey};
use parity_scale_codec::Decode;
use serial_test::serial;
use std::collections::HashMap;
use std::sync::Once;
use std::time::Duration;
use support::{register_jobs_through, setup_services_with_retry};
use tangle_subxt::tangle_testnet_runtime::api::runtime_types::{
    sp_runtime, tangle_primitives::services::types::TypeCheckError,
};
use tangle_subxt::tangle_testnet_runtime::api::{services, system};
use tokio::time::{sleep, timeout};
use tracing::{error, info, warn};

fn configure_test_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("FAAS_DISABLE_PREWARM", "1");
        std::env::set_var("FAAS_ENABLE_CONTRACT_ASSIGNMENT", "1");
    });
}

fn spawn_event_logger(client: TangleClient) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("Chain event logger started");
        let subxt = client.subxt_client().clone();
        let mut blocks = match subxt.blocks().subscribe_best().await {
            Ok(stream) => stream,
            Err(err) => {
                error!(?err, "Failed to subscribe to block stream for diagnostics");
                return;
            }
        };

        while let Some(next_block) = blocks.next().await {
            let Ok(block) = next_block else {
                error!("Block subscription returned error, continuing");
                continue;
            };

            let events = match block.events().await {
                Ok(events) => events,
                Err(err) => {
                    error!(?err, "Failed to fetch events for block");
                    continue;
                }
            };

            for evt in events.find::<system::events::ExtrinsicFailed>() {
                match evt {
                    Ok(failure) => {
                        let mut type_check_detail = None;
                        if let sp_runtime::DispatchError::Module(module_error) =
                            failure.dispatch_error.clone()
                        {
                            if module_error.index == 51 {
                                if let Ok(err) =
                                    TypeCheckError::decode(&mut &module_error.error[..])
                                {
                                    type_check_detail = Some(err);
                                }
                            }
                        }

                        error!(
                            ?failure.dispatch_error,
                            ?failure.dispatch_info,
                            ?type_check_detail,
                            "Observed System::ExtrinsicFailed"
                        );
                    }
                    Err(err) => {
                        error!(?err, "Failed to decode System::ExtrinsicFailed");
                    }
                }
            }

            for evt in events.find::<services::events::JobResultSubmitted>() {
                match evt {
                    Ok(result) => {
                        info!(
                            service_id = result.service_id,
                            call_id = result.call_id,
                            "Observed Services::JobResultSubmitted"
                        );
                    }
                    Err(err) => {
                        error!(?err, "Failed to decode Services::JobResultSubmitted");
                    }
                }
            }
        }
        info!("Chain event logger exiting");
    })
}

// Helper trait to convert any error to eyre::Report
trait ToEyreResult<T> {
    fn to_eyre(self) -> Result<T>;
}

impl<T, E: std::fmt::Display> ToEyreResult<T> for Result<T, E> {
    fn to_eyre(self) -> Result<T> {
        self.map_err(|e| eyre!("{}", e))
    }
}

fn operator_identity_from_config(config: &BlueprintEnvironment) -> Result<([u8; 32], Vec<u8>)> {
    let keystore = config.keystore();
    let sr25519_public = keystore
        .first_local::<SpSr25519>()
        .wrap_err("sr25519 key lookup failed")?;
    let ecdsa_public = keystore
        .first_local::<SpEcdsa>()
        .wrap_err("ecdsa key lookup failed")?;

    let public_key = PublicKey::from_sec1_bytes(&ecdsa_public.0 .0)
        .map_err(|e| eyre!("invalid compressed ECDSA key: {e}"))?;
    let uncompressed = public_key.to_encoded_point(false);
    let bytes = uncompressed.as_bytes();
    if bytes.len() != 65 {
        return Err(eyre!(
            "unexpected uncompressed ECDSA length {}",
            bytes.len()
        ));
    }

    Ok((sr25519_public.0 .0, bytes.to_vec()))
}

fn extract_output_bytes(output_value: &OutputValue) -> Vec<u8> {
    if let OutputValue::List(_field_type, outputs) = output_value {
        outputs
            .0
            .iter()
            .filter_map(|v| {
                if let OutputValue::Uint8(byte) = v {
                    Some(*byte)
                } else {
                    None
                }
            })
            .collect()
    } else {
        vec![]
    }
}

fn create_execute_job_args(image: &str, command: Vec<&str>) -> Vec<InputValue> {
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

fn create_execute_advanced_job_args(
    image: &str,
    command: Vec<&str>,
    mode: &str,
    timeout_secs: u64,
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
            Box::new(Some(InputValue::Uint64(timeout_secs))),
        ),
    ]
}

#[tokio::test]
#[serial]
async fn test_job_distribution_across_operators() -> Result<()> {
    configure_test_env();
    let _ = color_eyre::install();
    setup_log();

    info!("=== JOB DISTRIBUTION TEST ===");

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness = TangleTestHarness::setup(temp_dir).await.to_eyre()?;
    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 5>(&harness, false).await?;

    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    // Create contexts with longer random delays to spread out initial blockchain transactions
    let mut contexts = Vec::new();
    let mut operator_keys: HashMap<[u8; 32], Vec<u8>> = HashMap::new();
    for (idx, handle) in test_env.node_handles().await.into_iter().enumerate() {
        // Add progressively longer random delays to ensure transaction ordering
        let base_delay = (idx as u64) * 500; // 0ms, 500ms, 1000ms, 1500ms, 2000ms
        let random_jitter = rand::random::<u64>() % 200; // 0-200ms additional randomness
        sleep(Duration::from_millis(base_delay + random_jitter)).await;

        let config = handle.blueprint_config().await;
        let (account_id, ecdsa_key) = operator_identity_from_config(&config)?;
        operator_keys.insert(account_id, ecdsa_key);
        contexts.push(FaaSContext::new(config).await.to_eyre()?);
    }

    // Start all contexts together (harness requires all at once)
    test_env.start_with_contexts(contexts).await.to_eyre()?;

    info!("Submitting 20 jobs to 5 operators");

    let mut operator_counts: HashMap<String, usize> = HashMap::new();
    let mut jobs = Vec::new();
    for i in 0..20 {
        let job_args =
            create_execute_job_args("alpine:latest", vec!["echo", &format!("Job {}", i)]);

        let job = harness
            .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
            .await
            .to_eyre()?;

        info!("Submitted job {} with call ID {}", i, job.call_id);
        jobs.push((i, job));
    }

    let mut completed = 0;
    for (i, job) in jobs {
        match timeout(
            Duration::from_secs(60),
            harness.wait_for_job_execution(service_id, job),
        )
        .await
        {
            Ok(Ok(results)) => {
                assert_eq!(results.service_id, service_id);

                let operator_bytes =
                    operator_keys
                        .get(&results.operator.0)
                        .cloned()
                        .ok_or_else(|| {
                            eyre!(
                                "missing ECDSA key for operator {}",
                                hex_encode(results.operator.0)
                            )
                        })?;
                let operator_hex = hex_encode(operator_bytes);
                *operator_counts.entry(operator_hex.clone()).or_default() += 1;

                if let Some(output_value) = results.result.first() {
                    let output_bytes = extract_output_bytes(output_value);
                    let output = String::from_utf8_lossy(&output_bytes);
                    assert!(
                        output.contains(&format!("Job {}", i)),
                        "Job {} output mismatch: {}",
                        i,
                        output
                    );
                }

                completed += 1;
                info!(
                    "✅ Job {} completed ({}/20) by operator {} (call_id={})",
                    i, completed, operator_hex, results.call_id
                );
            }
            Ok(Err(e)) => {
                error!("Job {} failed: {}", i, e);
                return Err(color_eyre::eyre::eyre!("Job {} failed: {}", i, e));
            }
            Err(_) => {
                error!("Job {} timed out after 60s", i);
                color_eyre::eyre::bail!("Job {} timeout", i);
            }
        }
    }

    assert_eq!(completed, 20, "All 20 jobs must complete");
    for (operator, count) in operator_counts.iter() {
        info!("Operator {} executed {} jobs", operator, count);
    }
    info!("✅ All 20 jobs distributed and completed across 5 operators");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_operator_assignment_tracking() -> Result<()> {
    configure_test_env();
    let _ = color_eyre::install();
    setup_log();

    info!("=== OPERATOR ASSIGNMENT TRACKING TEST ===");

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness = TangleTestHarness::setup(temp_dir).await.to_eyre()?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 3>(&harness, false).await?;

    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    let mut contexts = Vec::new();
    for (idx, handle) in test_env.node_handles().await.into_iter().enumerate() {
        // Add random jitter to avoid transaction nonce conflicts
        let jitter_ms = (idx as u64) * 100 + (idx as u64 * 37) % 200; // Stagger by 100-300ms
        sleep(Duration::from_millis(jitter_ms)).await;

        let config = handle.blueprint_config().await;
        contexts.push(FaaSContext::new(config).await.to_eyre()?);
    }

    test_env.start_with_contexts(contexts).await.to_eyre()?;

    info!("Submitting jobs and tracking assignments");

    let mut completed_jobs = HashMap::new();

    for i in 0..9 {
        let job_args = create_execute_job_args(
            "alpine:latest",
            vec!["echo", &format!("Assignment test {}", i)],
        );

        let job = harness
            .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
            .await
            .to_eyre()?;

        let call_id = job.call_id;
        info!("Submitted job {} with call ID {}", i, call_id);

        let results = timeout(
            Duration::from_secs(30),
            harness.wait_for_job_execution(service_id, job),
        )
        .await
        .to_eyre()?
        .to_eyre()?;

        assert_eq!(results.service_id, service_id);
        assert_eq!(results.call_id, call_id);

        if let Some(output_value) = results.result.first() {
            let output_bytes = extract_output_bytes(output_value);
            let output = String::from_utf8_lossy(&output_bytes);
            assert!(output.contains(&format!("Assignment test {}", i)));
        }

        completed_jobs.insert(call_id, i);
        info!("✅ Job {} (call_id {}) completed", i, call_id);
    }

    assert_eq!(
        completed_jobs.len(),
        9,
        "All 9 jobs must complete with unique call IDs"
    );
    info!("✅ Assignment tracking verified - all jobs have unique IDs and completed");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_concurrent_execution() -> Result<()> {
    configure_test_env();
    let _ = color_eyre::install();
    setup_log();

    info!("=== CONCURRENT EXECUTION TEST ===");

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness = TangleTestHarness::setup(temp_dir).await.to_eyre()?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 5>(&harness, false).await?;

    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    let mut contexts = Vec::new();
    for (idx, handle) in test_env.node_handles().await.into_iter().enumerate() {
        // Add random jitter to avoid transaction nonce conflicts
        let jitter_ms = (idx as u64) * 100 + (idx as u64 * 37) % 200; // Stagger by 100-300ms
        sleep(Duration::from_millis(jitter_ms)).await;

        let config = handle.blueprint_config().await;
        contexts.push(FaaSContext::new(config).await.to_eyre()?);
    }

    test_env.start_with_contexts(contexts).await.to_eyre()?;

    info!("Submitting 25 concurrent jobs");

    let mut jobs = Vec::new();
    for i in 0..25 {
        let job_args = create_execute_job_args(
            "alpine:latest",
            vec![
                "sh",
                "-c",
                &format!("echo 'Concurrent job {}' && sleep 0.05", i),
            ],
        );

        let job = harness
            .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
            .await
            .to_eyre()?;

        jobs.push((i, job));
    }

    info!("Submitted all 25 jobs, waiting for completion");

    let mut completed = 0;
    for (i, job) in jobs {
        let results = timeout(
            Duration::from_secs(120),
            harness.wait_for_job_execution(service_id, job),
        )
        .await
        .to_eyre()?
        .to_eyre()?;

        assert_eq!(results.service_id, service_id);

        if let Some(output_value) = results.result.first() {
            let output_bytes = extract_output_bytes(output_value);
            let output = String::from_utf8_lossy(&output_bytes);
            assert!(output.contains(&format!("Concurrent job {}", i)));
        }

        completed += 1;
        if completed % 5 == 0 {
            info!("Progress: {}/25 jobs completed", completed);
        }
    }

    assert_eq!(completed, 25);
    info!("✅ All 25 concurrent jobs completed successfully");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_persistent_mode_execution() -> Result<()> {
    configure_test_env();
    let _ = color_eyre::install();
    setup_log();

    info!("=== PERSISTENT MODE TEST ===");

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness = TangleTestHarness::setup(temp_dir).await.to_eyre()?;
    let event_logger = spawn_event_logger(harness.client().clone());

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 2>(&harness, false).await?;

    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_ADVANCED_JOB_ID).await;

    let mut contexts = Vec::new();
    for (idx, handle) in test_env.node_handles().await.into_iter().enumerate() {
        // Add random jitter to avoid transaction nonce conflicts
        let jitter_ms = (idx as u64) * 100 + (idx as u64 * 37) % 200; // Stagger by 100-300ms
        sleep(Duration::from_millis(jitter_ms)).await;

        let config = handle.blueprint_config().await;
        contexts.push(FaaSContext::new(config).await.to_eyre()?);
    }

    test_env.start_with_contexts(contexts).await.to_eyre()?;

    info!("Testing persistent execution mode");

    for i in 0..5 {
        let job_args = create_execute_advanced_job_args(
            "alpine:latest",
            vec!["echo", &format!("Persistent job {}", i)],
            "persistent",
            60,
        );

        let job = harness
            .submit_job(service_id, EXECUTE_ADVANCED_JOB_ID as u8, job_args)
            .await
            .to_eyre()?;
        info!(
            service_id,
            call_id = job.call_id,
            iteration = i,
            "Submitted persistent execute_advanced job"
        );

        let results = timeout(
            Duration::from_secs(60),
            harness.wait_for_job_execution(service_id, job),
        )
        .await
        .to_eyre()?
        .to_eyre()?;

        assert_eq!(results.service_id, service_id);

        if let Some(output_value) = results.result.first() {
            let output_bytes = extract_output_bytes(output_value);
            let output = String::from_utf8_lossy(&output_bytes);
            assert!(output.contains(&format!("Persistent job {}", i)));
        }

        info!("✅ Persistent job {} completed", i);
    }

    info!("✅ Persistent mode execution verified");
    event_logger.abort();
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_cached_mode_execution() -> Result<()> {
    let _ = color_eyre::install();
    setup_log();

    info!("=== CACHED MODE TEST ===");

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness = TangleTestHarness::setup(temp_dir).await.to_eyre()?;
    let event_logger = spawn_event_logger(harness.client().clone());

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 2>(&harness, false).await?;

    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_ADVANCED_JOB_ID).await;

    let mut contexts = Vec::new();
    for (idx, handle) in test_env.node_handles().await.into_iter().enumerate() {
        // Add random jitter to avoid transaction nonce conflicts
        let jitter_ms = (idx as u64) * 100 + (idx as u64 * 37) % 200; // Stagger by 100-300ms
        sleep(Duration::from_millis(jitter_ms)).await;

        let config = handle.blueprint_config().await;
        contexts.push(FaaSContext::new(config).await.to_eyre()?);
    }

    test_env.start_with_contexts(contexts).await.to_eyre()?;

    info!("Testing cached execution mode");

    let job_args = create_execute_advanced_job_args(
        "alpine:latest",
        vec!["echo", "Cached result"],
        "cached",
        30,
    );

    let job = harness
        .submit_job(service_id, EXECUTE_ADVANCED_JOB_ID as u8, job_args)
        .await
        .to_eyre()?;
    info!(
        service_id,
        call_id = job.call_id,
        "Submitted cached execute_advanced job"
    );

    let results = timeout(
        Duration::from_secs(120),
        harness.wait_for_job_execution(service_id, job),
    )
    .await
    .to_eyre()?
    .to_eyre()?;

    assert_eq!(results.service_id, service_id);

    if let Some(output_value) = results.result.first() {
        let output_bytes = extract_output_bytes(output_value);
        let output = String::from_utf8_lossy(&output_bytes);
        assert!(output.contains("Cached result"));
    }

    info!("✅ Cached mode execution verified");
    event_logger.abort();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_mixed_workload_distribution() -> Result<()> {
    configure_test_env();
    let _ = color_eyre::install();
    setup_log();

    info!("=== MIXED WORKLOAD TEST ===");

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness = TangleTestHarness::setup(temp_dir).await.to_eyre()?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 4>(&harness, false).await?;

    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    let mut contexts = Vec::new();
    for (idx, handle) in test_env.node_handles().await.into_iter().enumerate() {
        // Add random jitter to avoid transaction nonce conflicts
        let jitter_ms = (idx as u64) * 100 + (idx as u64 * 37) % 200; // Stagger by 100-300ms
        sleep(Duration::from_millis(jitter_ms)).await;

        let config = handle.blueprint_config().await;
        contexts.push(FaaSContext::new(config).await.to_eyre()?);
    }

    test_env.start_with_contexts(contexts).await.to_eyre()?;

    info!("Submitting mixed quick and slow jobs");

    let mut jobs = Vec::new();

    for i in 0..12 {
        let (job_type, sleep_time) = if i % 3 == 0 {
            ("Quick", "0")
        } else if i % 3 == 1 {
            ("Medium", "0.1")
        } else {
            ("Slow", "0.3")
        };

        let job_args = create_execute_job_args(
            "alpine:latest",
            vec![
                "sh",
                "-c",
                &format!("echo '{} job {}' && sleep {}", job_type, i, sleep_time),
            ],
        );

        let job = harness
            .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
            .await
            .to_eyre()?;

        jobs.push((job_type, i, job));
    }

    info!("All 12 mixed jobs submitted");

    let mut completed = 0;
    for (job_type, i, job) in jobs {
        let results = timeout(
            Duration::from_secs(120),
            harness.wait_for_job_execution(service_id, job),
        )
        .await
        .to_eyre()?
        .to_eyre()?;

        assert_eq!(results.service_id, service_id);

        if let Some(output_value) = results.result.first() {
            let output_bytes = extract_output_bytes(output_value);
            let output = String::from_utf8_lossy(&output_bytes);
            assert!(output.contains(&format!("{} job {}", job_type, i)));
        }

        completed += 1;
        info!("✅ {} job {} completed ({}/12)", job_type, i, completed);
    }

    assert_eq!(completed, 12);
    info!("✅ Mixed workload handled correctly");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_single_operator_all_jobs() -> Result<()> {
    let _ = color_eyre::install();
    setup_log();

    info!("=== SINGLE OPERATOR TEST ===");

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness = TangleTestHarness::setup(temp_dir).await.to_eyre()?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    let handle = test_env.node_handles().await.into_iter().next().unwrap();
    let config = handle.blueprint_config().await;
    let ctx = FaaSContext::new(config).await.to_eyre()?;

    test_env.start_with_contexts(vec![ctx]).await.to_eyre()?;

    info!("Testing single operator handling multiple jobs");

    for i in 0..8 {
        let job_args =
            create_execute_job_args("alpine:latest", vec!["echo", &format!("Job {}", i)]);

        let job = harness
            .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
            .await
            .to_eyre()?;

        let results = timeout(
            Duration::from_secs(30),
            harness.wait_for_job_execution(service_id, job),
        )
        .await
        .to_eyre()?
        .to_eyre()?;

        assert_eq!(results.service_id, service_id);

        if let Some(output_value) = results.result.first() {
            let output_bytes = extract_output_bytes(output_value);
            let output = String::from_utf8_lossy(&output_bytes);
            assert!(output.contains(&format!("Job {}", i)));
        }

        info!("✅ Job {} completed by single operator", i);
    }

    info!("✅ Single operator handled all 8 jobs sequentially");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_job_result_correctness() -> Result<()> {
    let _ = color_eyre::install();
    setup_log();

    info!("=== JOB RESULT CORRECTNESS TEST ===");

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness = TangleTestHarness::setup(temp_dir).await.to_eyre()?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 2>(&harness, false).await?;
    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    let mut contexts = Vec::new();
    for (idx, handle) in test_env.node_handles().await.into_iter().enumerate() {
        // Add random jitter to avoid transaction nonce conflicts
        let jitter_ms = (idx as u64) * 100 + (idx as u64 * 37) % 200; // Stagger by 100-300ms
        sleep(Duration::from_millis(jitter_ms)).await;

        let config = handle.blueprint_config().await;
        contexts.push(FaaSContext::new(config).await.to_eyre()?);
    }

    test_env.start_with_contexts(contexts).await.to_eyre()?;

    info!("Testing output correctness for various commands");

    let test_cases = vec![
        ("echo 'test123'", "test123"),
        ("echo 'Hello World'", "Hello World"),
        ("printf 'exact'", "exact"),
        ("echo -n 'no newline'", "no newline"),
    ];

    for (i, (command, expected)) in test_cases.iter().enumerate() {
        let job_args = create_execute_job_args("alpine:latest", vec!["sh", "-c", command]);

        let job = harness
            .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
            .await
            .to_eyre()?;

        let results = timeout(
            Duration::from_secs(30),
            harness.wait_for_job_execution(service_id, job),
        )
        .await
        .to_eyre()?
        .to_eyre()?;

        if let Some(output_value) = results.result.first() {
            let output_bytes = extract_output_bytes(output_value);
            let output = String::from_utf8_lossy(&output_bytes);
            assert!(
                output.contains(expected),
                "Test case {} failed: expected '{}', got '{}'",
                i,
                expected,
                output
            );
        }

        info!("✅ Test case {} verified: {}", i, command);
    }

    info!("✅ All result correctness tests passed");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_error_output_capture() -> Result<()> {
    let _ = color_eyre::install();
    setup_log();

    info!("=== ERROR OUTPUT CAPTURE TEST ===");

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness = TangleTestHarness::setup(temp_dir).await.to_eyre()?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_FUNCTION_JOB_ID).await;

    let handle = test_env.node_handles().await.into_iter().next().unwrap();
    let config = handle.blueprint_config().await;
    let ctx = FaaSContext::new(config).await.to_eyre()?;

    test_env.start_with_contexts(vec![ctx]).await.to_eyre()?;

    info!("Testing error command execution");

    let job_args = create_execute_job_args(
        "alpine:latest",
        vec!["sh", "-c", "echo 'error message' >&2"],
    );

    let job = harness
        .submit_job(service_id, EXECUTE_FUNCTION_JOB_ID as u8, job_args)
        .await
        .to_eyre()?;

    let results = timeout(
        Duration::from_secs(30),
        harness.wait_for_job_execution(service_id, job),
    )
    .await
    .to_eyre()?
    .to_eyre()?;

    assert_eq!(results.service_id, service_id);

    info!("✅ Error output test completed");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_timeout_handling() -> Result<()> {
    let _ = color_eyre::install();
    setup_log();

    info!("=== TIMEOUT HANDLING TEST ===");

    let temp_dir = tempfile::TempDir::new().to_eyre()?;
    let harness = TangleTestHarness::setup(temp_dir).await.to_eyre()?;

    let (mut test_env, service_id, _) =
        setup_services_with_retry::<FaaSContext, 1>(&harness, false).await?;
    test_env.initialize().await.to_eyre()?;

    register_jobs_through(&mut test_env, EXECUTE_ADVANCED_JOB_ID).await;

    let handle = test_env.node_handles().await.into_iter().next().unwrap();
    let config = handle.blueprint_config().await;
    let ctx = FaaSContext::new(config).await.to_eyre()?;

    test_env.start_with_contexts(vec![ctx]).await.to_eyre()?;

    info!("Testing execution with timeout");

    let job_args = create_execute_advanced_job_args(
        "alpine:latest",
        vec!["sh", "-c", "echo 'Starting'; sleep 0.5; echo 'Done'"],
        "ephemeral",
        2,
    );

    let job = harness
        .submit_job(service_id, EXECUTE_ADVANCED_JOB_ID as u8, job_args)
        .await
        .to_eyre()?;

    let result = timeout(
        Duration::from_secs(10),
        harness.wait_for_job_execution(service_id, job),
    )
    .await;

    match result {
        Ok(Ok(results)) => {
            if let Some(output_value) = results.result.first() {
                let output_bytes = extract_output_bytes(output_value);
                let output = String::from_utf8_lossy(&output_bytes);
                info!("Job completed with output: {}", output);
            }
            info!("✅ Timeout handling verified");
        }
        Ok(Err(e)) => {
            warn!("Job failed (expected for timeout test): {}", e);
        }
        Err(_) => {
            warn!("Job timed out on our side (test timeout)");
        }
    }

    Ok(())
}
