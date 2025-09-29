//! Simple FaaS Platform Quickstart - Get Running in 2 Minutes
//!
//! This example shows the most direct path to using the FaaS platform.
//! Expert developers can immediately see the core API patterns.

use faas_client_sdk::{FaasClient, ExecuteRequest, Runtime};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Connect to platform
    let client = FaasClient::new("http://localhost:8080".to_string());

    // 2. Run a simple command (fastest path to results)
    let output = client.run("echo 'Hello, FaaS!'").await?;
    println!("Quick run: {}", output);

    // 3. Run Python code (no setup needed)
    let python_code = r#"
import json
data = {"result": 42, "status": "computed"}
print(json.dumps(data))
"#;
    let result = client.run_python(python_code).await?;
    println!("Python output: {}", result.stdout);

    // 4. Run JavaScript (instant execution)
    let js_code = r#"
const compute = () => ({ result: 42 * 2, timestamp: Date.now() });
console.log(JSON.stringify(compute()));
"#;
    let result = client.run_javascript(js_code).await?;
    println!("JavaScript output: {}", result.stdout);

    // 5. A/B test two approaches (parallel execution)
    let branches = vec![
        faas_client_sdk::ForkBranch {
            id: "fast".to_string(),
            command: "echo 'Fast algorithm'".to_string(),
            env_vars: None,
            weight: Some(0.5),
        },
        faas_client_sdk::ForkBranch {
            id: "accurate".to_string(),
            command: "sleep 0.1 && echo 'Accurate algorithm'".to_string(),
            env_vars: None,
            weight: Some(0.5),
        },
    ];

    let fork_result = client.fork_execution(
        branches,
        "alpine:latest",
        Some(faas_client_sdk::ForkStrategy::Fastest)
    ).await?;

    println!("Selected: {} ({})",
             fork_result.selected_branch.unwrap_or_default(),
             fork_result.selection_reason.unwrap_or_default());

    // 6. Use advanced features with explicit control
    let advanced_result = client.execute(ExecuteRequest {
        command: "python -c 'import sys; print(sys.version)'".to_string(),
        image: Some("python:3.11-slim".to_string()),
        runtime: Some(Runtime::Firecracker), // Use microVM for isolation
        env_vars: Some(vec![("ENV".to_string(), "production".to_string())]),
        working_dir: None,
        timeout_ms: Some(5000),
        cache_key: Some("python-version".to_string()), // Enable caching
    }).await?;

    println!("Advanced execution ({}ms): {}",
             advanced_result.duration_ms,
             advanced_result.stdout.trim());

    // 7. Check platform health
    let health = client.health().await?;
    println!("\nPlatform status: {}", health.status);

    Ok(())
}