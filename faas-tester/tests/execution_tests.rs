use color_eyre::eyre::{self, Result, WrapErr};
use docktopus::bollard::Docker;
use docktopus::DockerBuilder;
use dotenvy::dotenv;
use faas_common::{InvocationResult, SandboxConfig, SandboxExecutor};
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
fn get_firecracker_executor() -> Result<Arc<FirecrackerExecutor>> {
    let fc_bin = std::env::var("FAAS_FC_BINARY_PATH")
        .wrap_err("FAAS_FC_BINARY_PATH environment variable not set or path is invalid")?;
    let kernel = std::env::var("FAAS_FC_KERNEL_PATH")
        .wrap_err("FAAS_FC_KERNEL_PATH environment variable not set or path is invalid")?;

    // Add a check to ensure the paths are not the placeholder
    if fc_bin == "/path/to/your/firecracker" {
        return Err(eyre::eyre!(
            "FAAS_FC_BINARY_PATH is still the placeholder value. Please update .env"
        ));
    }

    let executor =
        FirecrackerExecutor::new(fc_bin.clone(), kernel.clone()).wrap_err_with(|| {
            format!(
                "Failed to create FirecrackerExecutor instance with bin: '{}', kernel: '{}'",
                fc_bin, kernel
            )
        })?;
    Ok(Arc::new(executor))
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
    let request = SandboxConfig {
        function_id: "tester-echo-exec".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), msg.to_string()],
        env_vars: None,
        payload: vec![],
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

    let request = SandboxConfig {
        function_id: "tester-exit-error-exec".to_string(),
        source: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo 'stderr message' >&2; exit 55".to_string(),
        ],
        env_vars: None,
        payload: vec![],
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

    let request = SandboxConfig {
        function_id: "tester-img-not-found-exec".to_string(),
        source: "docker.io/library/this-image-does-not-exist-hopefully:latest".to_string(),
        command: vec!["echo".to_string(), "hello".to_string()],
        env_vars: None,
        payload: vec![],
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
#[ignore] // Requires Firecracker setup and Env Vars: FAAS_FC_BINARY_PATH, FAAS_FC_KERNEL_PATH, FAAS_FC_ROOTFS_PATH
async fn test_firecracker_executor_echo() {
    dotenv().ok();

    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let executor = get_firecracker_executor().expect(
        "Failed to initialize Firecracker executor for test. \n
Please ensure FAAS_FC_BINARY_PATH and FAAS_FC_KERNEL_PATH are set correctly in your .env file and point to valid files."
    );

    let rootfs_path = std::env::var("FAAS_FC_ROOTFS_PATH").expect(
        "FAAS_FC_ROOTFS_PATH environment variable not set. Please set it in your .env file.",
    );

    let test_payload = b"Hello Firecracker Echo!".to_vec();

    let config = SandboxConfig {
        function_id: "fc-echo-test".to_string(),
        source: rootfs_path, // Path to the rootfs containing guest agent
        command: vec![],     // Command is ignored by guest agent receiving config via vsock
        env_vars: None,
        payload: test_payload.clone(),
    };

    println!("Executing Firecracker test with config: {:?}", config);
    let result = executor
        .execute(config)
        .await
        .expect("Firecracker execution failed");

    println!("Firecracker Result: {:?}", result);
    assert!(
        result.error.is_none(),
        "Error was not None: {:?}",
        result.error
    );
    // Assert that the response from the guest agent is the same as the payload sent
    assert_eq!(
        result.response.unwrap_or_default(),
        test_payload,
        "Payload and response do not match!"
    );
}

// TODO: Add tests for error cases (non-zero exit, invalid source, etc.) for both executors
