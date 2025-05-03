use blueprint_sdk::{
    prelude::*,
    tangle::{substrate::runtime_types::tangle_primitives::OutputValue, TangleTestHarness},
};
use color_eyre::eyre;
use faas_common::{ExecuteFunctionArgs, InvocationResult}; // Need InvocationResult to compare
use faas_lib::context::FaaSContext;
use faas_lib::jobs::execute_function_job;
use faas_lib::EXECUTE_FUNCTION_JOB_ID;
use std::env;
use std::time::Duration;
use tempfile::tempdir; // For setting env vars for tests

// Helper to setup harness and service with configured context
async fn setup_harness(
    executor_type: &str,
) -> eyre::Result<(TangleTestHarness, TangleServiceHandle)> {
    let temp_dir = tempdir()?;
    let harness = TangleTestHarness::builder().ephemeral(true).build().await?;
    let mut service_builder = harness.service_builder();

    // Set env vars for FaaSContext::new
    env::set_var("FAAS_EXECUTOR_TYPE", executor_type);
    // Set dummy paths for Docker, real paths needed for Firecracker if testing it
    env::set_var(
        "FC_BINARY_PATH",
        env::var("TEST_FC_BINARY_PATH").unwrap_or_else(|_| "/path/to/firecracker".into()),
    );
    env::set_var(
        "FC_KERNEL_PATH",
        env::var("TEST_FC_KERNEL_PATH").unwrap_or_else(|_| "/path/to/kernel.bin".into()),
    );
    // Rootfs path will be passed via job args

    let faas_context = FaaSContext::new(service_builder.env().clone()).await?;

    service_builder
        .route(
            EXECUTE_FUNCTION_JOB_ID,
            execute_function_job.layer(TangleLayer),
        )
        .context(faas_context);

    let service = service_builder.start(true).await?;
    Ok((harness, service))
}

#[tokio::test]
#[ignore] // Requires Docker running on host
async fn test_blueprint_docker_echo() -> eyre::Result<()> {
    let (harness, mut service) = setup_harness("docker").await?;

    let args = ExecuteFunctionArgs {
        image: "alpine:latest".to_string(), // Docker uses image field
        command: vec!["echo".to_string(), "Hello Docker Blueprint!".to_string()],
        env_vars: None,
        payload: vec![],
    };
    let encoded_args = args.encode();

    let job_call = harness
        .submit_job(service.service_id(), EXECUTE_FUNCTION_JOB_ID, encoded_args)
        .await?;

    let execution_result = harness
        .wait_for_job_execution(
            service.service_id(),
            &job_call,
            Some(Duration::from_secs(30)),
        )
        .await?;

    harness.verify_job_output(
        &execution_result,
        vec![OutputValue::Bytes(b"Hello Docker Blueprint!\n".to_vec())],
    );

    service.stop().await?;
    Ok(())
}

#[tokio::test]
#[ignore] // Requires Firecracker setup (Host env vars + rootfs/kernel)
async fn test_blueprint_firecracker_echo() -> eyre::Result<()> {
    // Check if required paths are set, otherwise skip
    let rootfs_path = match env::var("TEST_FC_ROOTFS_PATH") {
        Ok(p) => p,
        Err(_) => {
            println!("Skipping Firecracker blueprint test: TEST_FC_ROOTFS_PATH not set.");
            return Ok(()); // Skip test gracefully
        }
    };
    if env::var("TEST_FC_BINARY_PATH").is_err() || env::var("TEST_FC_KERNEL_PATH").is_err() {
        println!("Skipping Firecracker blueprint test: TEST_FC_BINARY_PATH or TEST_FC_KERNEL_PATH not set.");
        return Ok(()); // Skip test gracefully
    }

    let (harness, mut service) = setup_harness("firecracker").await?;

    let args = ExecuteFunctionArgs {
        image: rootfs_path, // Firecracker uses image field as source path
        command: vec!["/app/faas-guest-agent".to_string()], // Command likely ignored by agent
        env_vars: None,
        payload: b"Hello Firecracker Blueprint!".to_vec(),
    };
    let encoded_args = args.encode();

    let job_call = harness
        .submit_job(service.service_id(), EXECUTE_FUNCTION_JOB_ID, encoded_args)
        .await?;

    let execution_result = harness
        .wait_for_job_execution(
            service.service_id(),
            &job_call,
            Some(Duration::from_secs(30)),
        )
        .await?;

    // Verification depends on guest agent output via vsock
    println!("Firecracker Blueprint Result: {:?}", execution_result);
    // TODO: Add verification based on actual vsock communication result
    // harness.verify_job_output(
    //     &execution_result,
    //     vec![OutputValue::Bytes(b"Hello Firecracker Blueprint!".to_vec())],
    // );

    service.stop().await?;
    Ok(())
}

// TODO: Add more tests for error cases (non-zero exit, image not found, invalid rootfs)
