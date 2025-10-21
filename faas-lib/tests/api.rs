use faas_blueprint_lib::api_routes::build_api_router;
use faas_blueprint_lib::api_server::{
    ApiKeyPermissions, ApiServerConfig, ApiState, ExecuteRequest,
};
use faas_blueprint_lib::context::FaaSContext;
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use std::net::TcpListener as StdTcpListener;
use tempfile::tempdir;
use tokio::fs;
use tokio::sync::oneshot;

struct ApiTestServer {
    base_url: String,
    shutdown: oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
    _context: FaaSContext,
}

async fn spawn_api_server() -> Result<ApiTestServer, Box<dyn std::error::Error>> {
    std::env::set_var("FAAS_DISABLE_CONTRACT_ASSIGNMENT", "1");
    std::env::set_var("FAAS_DISABLE_PREWARM", "1");

    let temp_dir = tempdir()?;
    let base_path = temp_dir.path().to_path_buf();
    let keystore_dir = base_path.join("keystore");
    fs::create_dir_all(&keystore_dir).await?;

    let mut env = blueprint_sdk::runner::config::BlueprintEnvironment::default();
    env.test_mode = true;
    env.data_dir = base_path.clone();
    env.keystore_uri = keystore_dir.to_string_lossy().into_owned();

    let context = FaaSContext::new(env).await?;

    let listener = StdTcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);

    let mut config = ApiServerConfig::default();
    config.host = "127.0.0.1".to_string();
    config.port = port;
    config.api_keys.insert(
        "test-key".to_string(),
        ApiKeyPermissions {
            name: "test".to_string(),
            can_execute: true,
            can_manage_instances: true,
            rate_limit: Some(100),
        },
    );

    let state = ApiState {
        context: context.clone(),
        config: config.clone(),
        request_counts: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
    };

    let router = build_api_router(state);

    let listener = tokio::net::TcpListener::bind((config.host.as_str(), config.port)).await?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let server = axum::serve(listener, router).with_graceful_shutdown(async move {
        let _ = shutdown_rx.await;
    });

    let handle = tokio::spawn(async move {
        if let Err(err) = server.await {
            panic!("API server error: {err}");
        }
    });

    Ok(ApiTestServer {
        base_url: format!("http://127.0.0.1:{}", config.port),
        shutdown: shutdown_tx,
        handle,
        _context: context,
    })
}

#[tokio::test]
async fn test_api_server_execution() -> Result<(), Box<dyn std::error::Error>> {
    let server = spawn_api_server().await?;
    let client = Client::new();
    let base_url = &server.base_url;

    let health_response = client.get(format!("{}/health", base_url)).send().await?;
    assert_eq!(health_response.status(), 200);

    let execute_request = ExecuteRequest {
        image: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "Hello from API".to_string()],
        env_vars: None,
        payload: Vec::new(),
        mode: None,
        checkpoint_id: None,
        branch_from: None,
        timeout_secs: Some(30),
    };

    let execute_response = client
        .post(format!("{}/api/v1/execute", base_url))
        .header("x-api-key", "test-key")
        .json(&execute_request)
        .send()
        .await?;
    assert_eq!(execute_response.status(), 200);

    let result: serde_json::Value = execute_response.json().await?;
    assert!(result["request_id"].is_string());

    let unauth_response = client
        .post(format!("{}/api/v1/execute", base_url))
        .json(&execute_request)
        .send()
        .await?;
    assert!(unauth_response.status().is_client_error());

    let _ = server.shutdown.send(());
    let _ = server.handle.await;
    Ok(())
}

#[tokio::test]
async fn test_api_server_instances() -> Result<(), Box<dyn std::error::Error>> {
    let server = spawn_api_server().await?;
    let client = Client::new();
    let base_url = &server.base_url;

    let create_request = json!({
        "snapshot_id": null,
        "image": "alpine:latest",
        "cpu_cores": 1,
        "memory_mb": 512,
        "disk_gb": 4,
        "enable_ssh": false
    });

    let create_response = client
        .post(format!("{}/api/v1/instances", base_url))
        .header("x-api-key", "test-key")
        .json(&create_request)
        .send()
        .await?;
    assert_eq!(create_response.status(), 200);

    let instance: serde_json::Value = create_response.json().await?;
    let instance_id = instance["instance_id"]
        .as_str()
        .expect("instance id missing");
    assert_eq!(instance["status"], "running");

    let get_response = client
        .get(format!(
            "{}/api/v1/instances/{}/info",
            base_url, instance_id
        ))
        .header("x-api-key", "test-key")
        .send()
        .await?;
    assert_eq!(get_response.status(), 200);

    let stop_response = client
        .post(format!(
            "{}/api/v1/instances/{}/stop",
            base_url, instance_id
        ))
        .header("x-api-key", "test-key")
        .send()
        .await?;
    assert_eq!(stop_response.status(), 200);
    let stop_json: serde_json::Value = stop_response.json().await?;
    assert_eq!(stop_json["instance_id"], instance_id);
    assert!(stop_json["stopped"].as_bool().unwrap_or(false));

    let _ = server.shutdown.send(());
    let _ = server.handle.await;
    Ok(())
}
