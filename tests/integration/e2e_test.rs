/// End-to-end integration tests for the FaaS platform
/// These tests ACTUALLY run Docker containers and test the full stack

use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_gateway_server_starts() {
    // Start the gateway server
    let mut gateway = Command::new("cargo")
        .args(&["run", "--package", "faas-gateway-server", "--release"])
        .spawn()
        .expect("Failed to start gateway server");

    // Wait for it to be ready
    sleep(Duration::from_secs(3)).await;

    // Test health endpoint
    let response = reqwest::get("http://localhost:8080/health")
        .await
        .expect("Failed to connect to gateway");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    assert_eq!(body["status"], "healthy");

    // Cleanup
    gateway.kill().expect("Failed to stop gateway");
}

#[tokio::test]
async fn test_execute_simple_command() {
    // Start gateway
    let mut gateway = start_gateway_background();

    // Wait for startup
    sleep(Duration::from_secs(3)).await;

    // Execute a simple command
    let client = reqwest::Client::new();
    let response = client
        .post("http://localhost:8080/api/v1/execute")
        .json(&serde_json::json!({
            "command": "echo 'Hello from Docker'",
            "image": "alpine:latest"
        }))
        .send()
        .await
        .expect("Failed to execute command");

    assert_eq!(response.status(), 200);

    let result: serde_json::Value = response.json().await.expect("Failed to parse response");
    assert_eq!(result["exit_code"], 0);
    assert!(result["stdout"].as_str().unwrap().contains("Hello from Docker"));

    // Cleanup
    gateway.kill().expect("Failed to stop gateway");
}

#[tokio::test]
async fn test_execute_with_different_images() {
    let mut gateway = start_gateway_background();
    sleep(Duration::from_secs(3)).await;

    let client = reqwest::Client::new();

    // Test Python image
    let response = client
        .post("http://localhost:8080/api/v1/execute")
        .json(&serde_json::json!({
            "command": "python -c 'print(2 + 2)'",
            "image": "python:3.11-slim"
        }))
        .send()
        .await
        .expect("Failed to execute Python");

    let result: serde_json::Value = response.json().await.unwrap();
    assert_eq!(result["exit_code"], 0);
    assert_eq!(result["stdout"].as_str().unwrap().trim(), "4");

    // Test Node.js image
    let response = client
        .post("http://localhost:8080/api/v1/execute")
        .json(&serde_json::json!({
            "command": "node -e 'console.log(3 * 3)'",
            "image": "node:20-slim"
        }))
        .send()
        .await
        .expect("Failed to execute Node.js");

    let result: serde_json::Value = response.json().await.unwrap();
    assert_eq!(result["exit_code"], 0);
    assert_eq!(result["stdout"].as_str().unwrap().trim(), "9");

    gateway.kill().unwrap();
}

#[tokio::test]
async fn test_snapshot_lifecycle() {
    let mut gateway = start_gateway_background();
    sleep(Duration::from_secs(3)).await;

    let client = reqwest::Client::new();

    // Create a snapshot
    let response = client
        .post("http://localhost:8080/api/v1/snapshots")
        .json(&serde_json::json!({
            "name": "test-snapshot",
            "container_id": "test-container-123"
        }))
        .send()
        .await
        .expect("Failed to create snapshot");

    assert_eq!(response.status(), 200);
    let snapshot: serde_json::Value = response.json().await.unwrap();
    let snapshot_id = snapshot["id"].as_str().unwrap();

    // List snapshots
    let response = client
        .get("http://localhost:8080/api/v1/snapshots")
        .send()
        .await
        .expect("Failed to list snapshots");

    let snapshots: Vec<serde_json::Value> = response.json().await.unwrap();
    assert!(snapshots.iter().any(|s| s["id"] == snapshot_id));

    // Restore snapshot
    let response = client
        .post(format!("http://localhost:8080/api/v1/snapshots/{}/restore", snapshot_id))
        .send()
        .await
        .expect("Failed to restore snapshot");

    assert_eq!(response.status(), 200);

    gateway.kill().unwrap();
}

#[tokio::test]
async fn test_instance_management() {
    let mut gateway = start_gateway_background();
    sleep(Duration::from_secs(3)).await;

    let client = reqwest::Client::new();

    // Create an instance
    let response = client
        .post("http://localhost:8080/api/v1/instances")
        .json(&serde_json::json!({
            "image": "alpine:latest",
            "cpu_cores": 1,
            "memory_mb": 512
        }))
        .send()
        .await
        .expect("Failed to create instance");

    assert_eq!(response.status(), 200);
    let instance: serde_json::Value = response.json().await.unwrap();
    let instance_id = instance["id"].as_str().unwrap();

    // Get instance details
    let response = client
        .get(format!("http://localhost:8080/api/v1/instances/{}", instance_id))
        .send()
        .await
        .expect("Failed to get instance");

    assert_eq!(response.status(), 200);

    // Execute command in instance
    let response = client
        .post(format!("http://localhost:8080/api/v1/instances/{}/exec", instance_id))
        .json(&serde_json::json!({
            "command": "uname -a"
        }))
        .send()
        .await
        .expect("Failed to exec in instance");

    assert_eq!(response.status(), 200);
    let result: serde_json::Value = response.json().await.unwrap();
    assert_eq!(result["exit_code"], 0);

    // Stop instance
    let response = client
        .post(format!("http://localhost:8080/api/v1/instances/{}/stop", instance_id))
        .send()
        .await
        .expect("Failed to stop instance");

    assert_eq!(response.status(), 204);

    gateway.kill().unwrap();
}

#[tokio::test]
async fn test_execution_modes() {
    let mut gateway = start_gateway_background();
    sleep(Duration::from_secs(3)).await;

    let client = reqwest::Client::new();

    // Test different execution modes
    let modes = vec!["ephemeral", "cached", "persistent"];

    for mode in modes {
        let response = client
            .post("http://localhost:8080/api/v1/execute/advanced")
            .json(&serde_json::json!({
                "command": "date",
                "image": "alpine:latest",
                "mode": mode
            }))
            .send()
            .await
            .expect(&format!("Failed to execute in {} mode", mode));

        assert_eq!(response.status(), 200, "Mode {} failed", mode);
        let result: serde_json::Value = response.json().await.unwrap();
        assert_eq!(result["exit_code"], 0);
    }

    gateway.kill().unwrap();
}

#[tokio::test]
async fn test_error_handling() {
    let mut gateway = start_gateway_background();
    sleep(Duration::from_secs(3)).await;

    let client = reqwest::Client::new();

    // Test command that fails
    let response = client
        .post("http://localhost:8080/api/v1/execute")
        .json(&serde_json::json!({
            "command": "exit 1",
            "image": "alpine:latest"
        }))
        .send()
        .await
        .expect("Failed to execute command");

    assert_eq!(response.status(), 200);
    let result: serde_json::Value = response.json().await.unwrap();
    assert_eq!(result["exit_code"], 1);

    // Test invalid image
    let response = client
        .post("http://localhost:8080/api/v1/execute")
        .json(&serde_json::json!({
            "command": "echo test",
            "image": "this-image-does-not-exist:latest"
        }))
        .send()
        .await
        .expect("Failed to send request");

    // Should return error status
    assert_eq!(response.status(), 500);

    gateway.kill().unwrap();
}

#[tokio::test]
async fn test_concurrent_executions() {
    let mut gateway = start_gateway_background();
    sleep(Duration::from_secs(3)).await;

    let client = reqwest::Client::new();

    // Launch multiple concurrent executions
    let mut tasks = vec![];
    for i in 0..5 {
        let client = client.clone();
        let task = tokio::spawn(async move {
            let response = client
                .post("http://localhost:8080/api/v1/execute")
                .json(&serde_json::json!({
                    "command": format!("echo 'Task {}'", i),
                    "image": "alpine:latest"
                }))
                .send()
                .await
                .expect("Failed to execute");

            let result: serde_json::Value = response.json().await.unwrap();
            (i, result)
        });
        tasks.push(task);
    }

    // Wait for all to complete
    for task in tasks {
        let (i, result) = task.await.unwrap();
        assert_eq!(result["exit_code"], 0);
        assert!(result["stdout"].as_str().unwrap().contains(&format!("Task {}", i)));
    }

    gateway.kill().unwrap();
}

// Helper function to start gateway in background
fn start_gateway_background() -> std::process::Child {
    Command::new("cargo")
        .args(&["run", "--package", "faas-gateway-server", "--release"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("Failed to start gateway server")
}