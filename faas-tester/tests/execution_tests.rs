use color_eyre::eyre::{self, Result};
use docktopus::bollard::Docker;
use docktopus::DockerBuilder;
use faas_common::{ExecutionRequest, Executor, InvocationResult, SandboxConfig, SandboxExecutor};
use faas_executor::{firecracker::FirecrackerExecutor, DockerExecutor};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

// Helper to get a Docker client for tests
async fn get_test_docker_client() -> Result<Arc<Docker>> {
    let builder = DockerBuilder::new().await.map_err(|e| {
        eyre::eyre!(
            "Failed to create DockerBuilder for test: {}. Is Docker running?",
            e
        )
    })?;
    Ok(builder.client())
}

// Helper for Docker tests (similar to orchestrator tests)
async fn get_docker_executor() -> Arc<DockerExecutor> {
    let builder = DockerBuilder::new()
        .await
        .expect("Failed to build Docker client for test");
    Arc::new(DockerExecutor::new(builder.client()))
}

// Helper for Firecracker tests (requires config)
fn get_firecracker_executor() -> Option<Arc<FirecrackerExecutor>> {
    let fc_bin = std::env::var("TEST_FC_BINARY_PATH").ok()?;
    let kernel = std::env::var("TEST_FC_KERNEL_PATH").ok()?;
    let executor = FirecrackerExecutor::new(fc_bin, kernel).ok()?;
    Some(Arc::new(executor))
}

#[tokio::test]
#[ignore] // Keep ignored by default, run manually with --include-ignored
async fn test_executor_execute_echo_success() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
    let docker_client = get_test_docker_client().await?;
    let executor = DockerExecutor::new(docker_client);

    let msg = "Executor execute test success!";
    let request = ExecutionRequest {
        image: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), msg.to_string()],
        env_vars: None,
        function_id: "tester-echo-exec".to_string(),
    };

    // Call the trait method
    let result: InvocationResult = executor.execute(request).await?;

    assert!(
        result.error.is_none(),
        "Expected no error, got: {:?}",
        result.error
    );
    let expected_output = format!("{}\n", msg);
    assert_eq!(
        result.response.as_deref().unwrap(),
        expected_output.as_bytes()
    );
    assert!(result.logs.unwrap_or_default().contains(msg));
    Ok(())
}

#[tokio::test]
#[ignore] // Keep ignored by default
async fn test_executor_execute_exit_error() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
    let docker_client = get_test_docker_client().await?;
    let executor = DockerExecutor::new(docker_client);

    let request = ExecutionRequest {
        image: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo 'stderr message' >&2; exit 55".to_string(),
        ],
        env_vars: None,
        function_id: "tester-exit-error-exec".to_string(),
    };

    // Call the trait method - it should return Ok(InvocationResult { error: Some(...) })
    let result: InvocationResult = executor.execute(request).await?;

    assert!(result.response.is_none(), "Expected no response on error");
    assert!(result.error.is_some(), "Expected an error message");
    let error_msg = result.error.unwrap();
    let expected_error_part = "Container failed with exit code: 55";
    assert!(
        error_msg.contains(expected_error_part),
        "Error mismatch: Expected '{}', got '{}'",
        expected_error_part,
        error_msg
    );
    assert!(
        error_msg.contains("stderr message"),
        "Stderr message missing from logs in error: '{}'",
        error_msg
    );
    assert!(result.logs.unwrap_or_default().contains("stderr message"));
    Ok(())
}

#[tokio::test]
#[ignore] // Keep ignored by default
async fn test_executor_execute_image_not_found() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
    let docker_client = get_test_docker_client().await?;
    let executor = DockerExecutor::new(docker_client);

    let request = ExecutionRequest {
        image: "docker.io/library/this-image-does-not-exist-hopefully:latest".to_string(),
        command: vec!["echo".to_string(), "hello".to_string()],
        env_vars: None,
        function_id: "tester-img-not-found-exec".to_string(),
    };

    // Call the trait method - it should return Err(FaasError::Executor(...))
    let result = executor.execute(request).await;

    assert!(
        result.is_err(),
        "Expected execute to return an error for image not found"
    );
    let error_string = result.err().unwrap().to_string();
    // Check the FaasError::Executor variant string representation
    assert!(
        error_string.contains("Executor Error"),
        "Expected FaasError::Executor variant, got: {}",
        error_string
    );
    assert!(
        error_string.contains("Container creation failed"),
        "Expected creation failed error, got: {}",
        error_string
    );
    assert!(
        error_string.contains("No such image") || error_string.contains("404"),
        "Expected image not found details, got: {}",
        error_string
    );
    Ok(())
}

#[tokio::test]
#[ignore] // Requires Docker
async fn test_docker_executor_echo() {
    let executor = get_docker_executor().await;
    let config = SandboxConfig {
        function_id: "docker-echo-test".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "Hello Docker!".to_string()],
        env_vars: None,
        payload: vec![],
    };
    let result = executor.execute(config).await.expect("Execution failed");
    assert!(
        result.error.is_none(),
        "Error was not None: {:?}",
        result.error
    );
    assert_eq!(result.response.unwrap_or_default(), b"Hello Docker!\n");
}

#[tokio::test]
#[ignore] // Requires Firecracker setup and Env Vars: TEST_FC_BINARY_PATH, TEST_FC_KERNEL_PATH, TEST_FC_ROOTFS_PATH
async fn test_firecracker_executor_echo() {
    let executor = match get_firecracker_executor() {
        Some(exec) => exec,
        None => {
            println!("Skipping Firecracker test: TEST_FC_BINARY_PATH or TEST_FC_KERNEL_PATH not set or executor creation failed.");
            return;
        }
    };
    let rootfs_path = match std::env::var("TEST_FC_ROOTFS_PATH") {
        Ok(p) => p,
        Err(_) => {
            println!("Skipping Firecracker test: TEST_FC_ROOTFS_PATH not set.");
            return;
        }
    };

    // Assuming rootfs contains guest agent that echoes args/payload
    let config = SandboxConfig {
        function_id: "fc-echo-test".to_string(),
        source: rootfs_path, // Path to the rootfs containing guest agent
        // Command might be ignored by guest agent if it takes config via vsock
        command: vec!["/app/faas-guest-agent".to_string()], // Or the command to run directly if agent isn't primary entrypoint
        env_vars: None,
        payload: b"Hello Firecracker!".to_vec(),
    };

    let result = executor.execute(config).await.expect("Execution failed");

    // Verification depends heavily on guest agent implementation
    println!("Firecracker Result: {:?}", result);
    assert!(
        result.error.is_none(),
        "Error was not None: {:?}",
        result.error
    );
    // TODO: Assert based on actual expected output from guest agent via vsock
    // assert_eq!(result.response.unwrap_or_default(), b"Hello Firecracker!");
}

// TODO: Add tests for error cases (non-zero exit, invalid source, etc.) for both executors
