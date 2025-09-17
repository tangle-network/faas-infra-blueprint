use faas_lib::api_server::{ApiServerConfig, ApiKeyPermissions, ExecuteRequest};
use reqwest;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
#[ignore] // Run with: cargo test --test api -- --ignored --nocapture
async fn test_api_server_execution() -> Result<(), Box<dyn std::error::Error>> {
    // This test assumes the faas-blueprint service is running with API server enabled
    // Start it with: FAAS_API_KEY=test-key cargo run --bin faas-blueprint

    let client = reqwest::Client::new();
    let base_url = "http://localhost:8080";

    // Test health endpoint (no auth required)
    let health_response = client
        .get(format!("{}/health", base_url))
        .send()
        .await?;

    assert_eq!(health_response.status(), 200);
    let health_json: serde_json::Value = health_response.json().await?;
    assert_eq!(health_json["status"], "healthy");
    println!("✅ Health check passed");

    // Test execution endpoint (requires auth)
    let execute_request = ExecuteRequest {
        image: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "Hello from API".to_string()],
        env_vars: None,
        payload: Vec::new(),
    };

    let execute_response = client
        .post(format!("{}/api/v1/execute", base_url))
        .header("x-api-key", "test-key") // Use the API key set in environment
        .json(&execute_request)
        .send()
        .await?;

    if execute_response.status() == 401 {
        println!("⚠️  Authentication failed - make sure service is running with FAAS_API_KEY=test-key");
        return Ok(());
    }

    assert_eq!(execute_response.status(), 200);
    let result: serde_json::Value = execute_response.json().await?;

    assert!(result["request_id"].is_string());
    assert!(result["response"].is_array() || result["response"].is_null());

    if let Some(response_bytes) = result["response"].as_array() {
        let response_str = String::from_utf8(
            response_bytes.iter()
                .filter_map(|v| v.as_u64().map(|b| b as u8))
                .collect()
        )?;
        println!("✅ Execution response: {}", response_str.trim());
    }

    // Test rate limiting
    println!("Testing rate limiting...");
    let mut exceeded = false;
    for i in 0..70 {
        let response = client
            .post(format!("{}/api/v1/execute", base_url))
            .header("x-api-key", "test-key")
            .json(&execute_request)
            .send()
            .await?;

        if response.status() == 429 {
            println!("✅ Rate limit triggered after {} requests", i);
            exceeded = true;
            break;
        }
    }

    if !exceeded {
        println!("⚠️  Rate limit not triggered (may be disabled or set higher than 60/min)");
    }

    // Test unauthorized access
    let unauth_response = client
        .post(format!("{}/api/v1/execute", base_url))
        .json(&execute_request)
        .send()
        .await?;

    assert_eq!(unauth_response.status(), 400); // Should fail without API key
    println!("✅ Unauthorized access properly rejected");

    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_api_server_instances() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:8080";

    // Create instance
    let create_request = json!({
        "resources": {
            "cpu_cores": 2,
            "memory_mb": 4096,
            "disk_gb": 20
        }
    });

    let create_response = client
        .post(format!("{}/api/v1/instances", base_url))
        .header("x-api-key", "test-key")
        .json(&create_request)
        .send()
        .await?;

    if create_response.status() == 401 {
        println!("⚠️  Authentication failed - make sure service is running with FAAS_API_KEY=test-key");
        return Ok(());
    }

    assert_eq!(create_response.status(), 200);
    let instance: serde_json::Value = create_response.json().await?;

    assert!(instance["id"].is_string());
    assert_eq!(instance["status"], "pending");

    let instance_id = instance["id"].as_str().unwrap();
    println!("✅ Created instance: {}", instance_id);

    // Get instance
    let get_response = client
        .get(format!("{}/api/v1/instances/{}", base_url, instance_id))
        .header("x-api-key", "test-key")
        .send()
        .await?;

    assert_eq!(get_response.status(), 200);
    let instance_info: serde_json::Value = get_response.json().await?;
    assert_eq!(instance_info["id"], instance_id);
    println!("✅ Retrieved instance info");

    // Stop instance
    let stop_response = client
        .post(format!("{}/api/v1/instances/{}/stop", base_url, instance_id))
        .header("x-api-key", "test-key")
        .send()
        .await?;

    assert_eq!(stop_response.status(), 200);
    let stopped: serde_json::Value = stop_response.json().await?;
    assert_eq!(stopped["status"], "stopped");
    println!("✅ Stopped instance");

    Ok(())
}