use faas_common::InvocationResult;
use faas_executor::{docktopus::DockerBuilder, DockerExecutor};
use faas_gateway::create_axum_router;
use faas_orchestrator::Orchestrator;
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

// Helper to spawn server in background
async fn spawn_app() -> std::result::Result<
    (String, tokio::task::JoinHandle<()>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let builder = DockerBuilder::new()
        .await
        .map_err(|e| format!("Failed to create DockerBuilder: {}", e))?;
    let executor = Arc::new(DockerExecutor::new(builder.client()));
    let orchestrator = Arc::new(Orchestrator::new(executor));
    let app = create_axum_router(orchestrator);
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server_url = format!("http://{}", addr);
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Ok((server_url, server_handle))
}

#[tokio::test]
#[ignore] // Requires Docker
async fn test_integration_invoke_success(
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (server_url, server_handle) = spawn_app().await?;
    let client = Client::new();
    let function_id = "integ-test-echo";

    let req_body = json!({
        "image": "alpine:latest",
        "command": ["echo", "Hello Integration"],
        "env_vars": null
    });

    let invoke_url = format!("{}/functions/{}/invoke", server_url, function_id);

    // Send request
    let response = client.post(&invoke_url).json(&req_body).send().await?;

    // Check status
    assert!(
        response.status().is_success(),
        "Request failed: {}",
        response.status()
    );

    // Check body
    let invocation_result: InvocationResult = response.json().await?;
    assert!(
        invocation_result.error.is_none(),
        "Expected no execution error, got: {:?}",
        invocation_result.error
    );
    assert_eq!(
        invocation_result.response.unwrap_or_default(),
        b"Hello Integration\n"
    );
    assert!(invocation_result
        .logs
        .unwrap_or_default()
        .contains("Hello Integration"));

    // Shutdown server (optional, depends on test runner behavior)
    server_handle.abort();
    Ok(())
}

#[tokio::test]
#[ignore] // Requires Docker
async fn test_integration_invoke_execution_error() -> Result<(), Box<dyn StdError + Send + Sync>> {
    let (server_url, server_handle) = spawn_app().await?;
    let client = Client::new();
    let function_id = "integ-test-fail";
    let req_body = json!({
        "image": "alpine:latest",
        "command": ["sh", "-c", "echo stderr orch >&2 && exit 7"],
        "env_vars": null
    });
    let invoke_url = format!("{}/functions/{}/invoke", server_url, function_id);
    let response = client.post(&invoke_url).json(&req_body).send().await?;

    assert_eq!(
        response.status(),
        reqwest::StatusCode::INTERNAL_SERVER_ERROR
    );

    let error_body: HashMap<String, String> = response.json().await?;
    let error_message = error_body.get("error").ok_or_else(|| {
        Box::<dyn StdError + Send + Sync>::from("Missing 'error' key in response")
    })?;

    // Assert the error message structure coming DIRECTLY from InvocationResult.error
    let expected_prefix = "Container failed with exit code: 7"; // Actual message start
    assert!(
        error_message.starts_with(expected_prefix),
        "Prefix mismatch: Expected '{}', got: '{}'",
        expected_prefix,
        error_message
    );
    assert!(
        error_message.contains("stderr orch"),
        "Stderr mismatch: {}",
        error_message
    );

    server_handle.abort();
    Ok(())
}

#[tokio::test]
#[ignore] // Requires Docker
async fn test_integration_invoke_image_not_found() -> Result<(), Box<dyn StdError + Send + Sync>> {
    let (server_url, server_handle) = spawn_app().await?;
    let client = Client::new();
    let function_id = "integ-test-notfound";
    let req_body = json!({
        "image": "docker.io/library/image-that-does-not-exist-at-all:latest",
        "command": ["echo", "hello"],
        "env_vars": null
    });
    let invoke_url = format!("{}/functions/{}/invoke", server_url, function_id);
    let response = client.post(&invoke_url).json(&req_body).send().await?;

    assert_eq!(
        response.status(),
        reqwest::StatusCode::INTERNAL_SERVER_ERROR
    );

    let error_body: HashMap<String, String> = response.json().await?;
    let error_message = error_body.get("error").ok_or_else(|| {
        Box::<dyn StdError + Send + Sync>::from("Missing 'error' key in response")
    })?;

    // Assert the error message structure
    let expected_prefix = "Executor Error: Container creation failed"; // Specific prefix
    assert!(
        error_message.starts_with(expected_prefix),
        "Prefix mismatch: Expected '{}', got: '{}'",
        expected_prefix,
        error_message
    );
    assert!(
        error_message.contains("No such image") || error_message.contains("404"),
        "Image not found detail mismatch: {}",
        error_message
    );

    server_handle.abort();
    Ok(())
}
