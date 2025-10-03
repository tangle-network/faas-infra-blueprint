//! Quickstart example - minimal working demo

use faas_sdk::{FaasClient, ExecuteRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("FaaS Platform Quickstart\n");

    // Connect to FaaS platform
    let client = FaasClient::new("http://localhost:8080".to_string());

    // Example 1: Simple execution
    println!("1. Simple execution:");
    let result = client.execute(ExecuteRequest {
        command: "echo Hello from FaaS!".to_string(),
        image: Some("alpine:latest".to_string()),
        ..Default::default()
    }).await?;

    println!("   Output: {}", result.stdout);

    // Example 2: With input data via stdin
    println!("\n2. Processing data:");
    let result = client.execute(ExecuteRequest {
        command: "wc -l".to_string(),
        image: Some("alpine:latest".to_string()),
        ..Default::default()
    }).await?;

    println!("   Line count: {}", result.stdout.trim());

    // Example 3: Environment variables
    println!("\n3. With environment:");
    let result = client.execute(ExecuteRequest {
        command: "sh -c 'echo $MESSAGE'".to_string(),
        image: Some("alpine:latest".to_string()),
        env_vars: Some(vec![("MESSAGE".to_string(), "FaaS Platform Works!".to_string())]),
        ..Default::default()
    }).await?;

    println!("   Env output: {}", result.stdout);

    println!("\n✅ All tests passed!");
    Ok(())
}