//! Comprehensive integration tests for FaaS Rust SDK
//!
//! Tests all documented top-level API methods:
//! - execute, run_python, run_javascript, run_bash, fork_execution
//! - prewarm, get_metrics, health_check

use faas_client_sdk::*;
use mockito::Server;
use serde_json::json;
use tokio;

#[tokio::test]
async fn test_execute_basic() {
    let mut server = Server::new();
    let mock = server.mock("POST", "/api/v1/execute")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "stdout": "Hello from FaaS!",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 45,
            "request_id": "test-123"
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let result = client.execute(ExecuteRequest {
        command: "echo 'Hello from FaaS!'".to_string(),
        image: Some("alpine:latest".to_string()),
        runtime: None,
        env_vars: None,
        working_dir: None,
        timeout_ms: None,
        cache_key: None,
    }).await.unwrap();

    assert_eq!(result.stdout, "Hello from FaaS!");
    assert_eq!(result.exit_code, 0);
    assert!(result.duration_ms < 100);
    mock.assert();
}

#[tokio::test]
async fn test_run_python() {
    let mut server = Server::new();
    let mock = server.mock("POST", "/api/v1/execute")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "stdout": "Hello from Python!\n42",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 67,
            "request_id": "python-test-456"
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let code = r#"
print("Hello from Python!")
result = 40 + 2
print(result)
"#;

    let result = client.run_python(code).await.unwrap();
    assert!(result.stdout.contains("Hello from Python!"));
    assert!(result.stdout.contains("42"));
    assert_eq!(result.exit_code, 0);
    mock.assert();
}

#[tokio::test]
async fn test_run_javascript() {
    let mut server = Server::new();
    let mock = server.mock("POST", "/api/v1/execute")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "stdout": "Hello from JavaScript!\n42",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 55,
            "request_id": "js-test-789"
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let code = r#"console.log("Hello from JavaScript!"); console.log(42);"#;
    let result = client.run_javascript(code).await.unwrap();

    assert!(result.stdout.contains("Hello from JavaScript!"));
    assert!(result.stdout.contains("42"));
    assert_eq!(result.exit_code, 0);
    mock.assert();
}

#[tokio::test]
async fn test_run_bash() {
    let mut server = Server::new();
    let mock = server.mock("POST", "/api/v1/execute")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "stdout": "Hello from Bash!\nCurrent date: 2024-01-15",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 30,
            "request_id": "bash-test-101"
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let script = r#"echo "Hello from Bash!"; echo "Current date: $(date +%Y-%m-%d)""#;
    let result = client.run_bash(script).await.unwrap();

    assert!(result.stdout.contains("Hello from Bash!"));
    assert_eq!(result.exit_code, 0);
    mock.assert();
}

#[tokio::test]
async fn test_prewarm() {
    let mut server = Server::new();
    let mock = server.mock("POST", "/api/v1/prewarm")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "message": "Pre-warmed 3 containers",
            "containers_created": 3
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let result = client.prewarm("python:3.11-slim", 3).await;
    assert!(result.is_ok());
    mock.assert();
}

#[tokio::test]
async fn test_get_metrics() {
    let mut server = Server::new();
    let mock = server.mock("GET", "/api/v1/metrics")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "total_executions": 1547,
            "avg_execution_time_ms": 87.5,
            "cache_hit_rate": 0.73,
            "active_containers": 15,
            "active_instances": 5,
            "memory_usage_mb": 2048,
            "cpu_usage_percent": 45.3
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let result = client.get_metrics().await.unwrap();
    assert!(result.total_executions > 0);
    assert!(result.avg_execution_time_ms < 200.0);
    mock.assert();
}

#[tokio::test]
async fn test_health_check() {
    let mut server = Server::new();
    let mock = server.mock("GET", "/health")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "status": "healthy",
            "version": "1.0.0",
            "uptime_seconds": 86400,
            "components": {
                "executor": "healthy",
                "docker": "healthy",
                "cache": "healthy"
            }
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let result = client.health_check().await.unwrap();
    assert_eq!(result.status, "healthy");
    mock.assert();
}

#[tokio::test]
async fn test_fork_execution() {
    let mut server = Server::new();
    let mock = server.mock("POST", "/api/v1/execute-advanced")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "stdout": "Forked execution result",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 25,
            "request_id": "fork-test-202"
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let result = client.fork_execution("parent-123", "echo 'Forked execution'").await.unwrap();
    assert!(result.stdout.contains("Forked execution"));
    assert_eq!(result.exit_code, 0);
    mock.assert();
}

#[tokio::test]
async fn test_execute_with_env_vars() {
    let mut server = Server::new();
    let mock = server.mock("POST", "/api/v1/execute")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "stdout": "TEST_VAR=production",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 35,
            "request_id": "env-test-303"
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let mut env_vars = std::collections::HashMap::new();
    env_vars.insert("TEST_VAR".to_string(), "production".to_string());

    let result = client.execute(ExecuteRequest {
        command: "echo TEST_VAR=$TEST_VAR".to_string(),
        image: Some("alpine:latest".to_string()),
        runtime: None,
        env_vars: Some(env_vars.into_iter().collect()),
        working_dir: None,
        timeout_ms: None,
        cache_key: None,
    }).await.unwrap();

    assert!(result.stdout.contains("TEST_VAR=production"));
    mock.assert();
}

#[tokio::test]
async fn test_execute_with_working_dir() {
    let mut server = Server::new();
    let mock = server.mock("POST", "/api/v1/execute")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "stdout": "/app",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 20,
            "request_id": "workdir-test-404"
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let result = client.execute(ExecuteRequest {
        command: "pwd".to_string(),
        image: Some("alpine:latest".to_string()),
        runtime: None,
        env_vars: None,
        working_dir: Some("/app".to_string()),
        timeout_ms: None,
        cache_key: None,
    }).await.unwrap();

    assert!(result.stdout.contains("/app"));
    mock.assert();
}

#[tokio::test]
async fn test_error_handling() {
    let mut server = Server::new();
    let mock = server.mock("POST", "/api/v1/execute")
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "error": "Internal server error",
            "details": "Container failed to start"
        }).to_string())
        .create();

    let client = FaasClient::new(server.url());

    let result = client.execute(ExecuteRequest {
        command: "exit 1".to_string(),
        image: Some("alpine:latest".to_string()),
        runtime: None,
        env_vars: None,
        working_dir: None,
        timeout_ms: None,
        cache_key: None,
    }).await;

    assert!(result.is_err());
    mock.assert();
}