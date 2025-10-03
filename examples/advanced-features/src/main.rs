//! Advanced FaaS Platform Features Demo
//!
//! This example demonstrates the advanced capabilities of the FaaS platform,
//! including multi-language execution, caching, and parallel execution.

use faas_sdk::{FaasClient, ExecuteRequest, Runtime};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 FaaS Platform Advanced Features Demo\n");

    // 1. Connect to platform
    let client = FaasClient::new("http://localhost:8080".to_string());

    // 2. Run a simple command (fastest path to results)
    println!("1. Quick command execution:");
    let output = client.run("echo 'Hello from FaaS!'").await?;
    println!("   Output: {}", output);

    // 3. Run Python code (no setup needed)
    println!("\n2. Python execution:");
    let python_code = r#"
import json
import sys
data = {
    "result": 42,
    "status": "computed",
    "python_version": sys.version.split()[0]
}
print(json.dumps(data, indent=2))
"#;
    let result = client.run_python(python_code).await?;
    println!("   Python output: {}", result.stdout.trim());

    // 4. Run JavaScript (instant execution)
    println!("\n3. JavaScript execution:");
    let js_code = r#"
const compute = () => ({
    result: 42 * 2,
    timestamp: new Date().toISOString(),
    node_version: process.version
});
console.log(JSON.stringify(compute(), null, 2));
"#;
    let result = client.run_javascript(js_code).await?;
    println!("   JavaScript output: {}", result.stdout.trim());

    // 5. Fork execution from a base
    println!("\n4. Forked execution:");
    // First create a base execution
    let base = client.execute(ExecuteRequest {
        command: "echo 'Base execution established'".to_string(),
        image: Some("alpine:latest".to_string()),
        ..Default::default()
    }).await?;

    // Fork from the base
    let fork_result = client.fork_execution(
        &base.request_id,
        "echo 'Forked execution completed'"
    ).await?;
    println!("   Fork result: {}", fork_result.stdout.trim());

    // 6. Use advanced features with explicit control
    println!("\n5. Advanced execution with full control:");
    let advanced_result = client.execute(ExecuteRequest {
        command: "python -c 'import sys; print(f\"Python {sys.version}\")'".to_string(),
        image: Some("python:3.11-slim".to_string()),
        runtime: Some(Runtime::Docker), // Explicitly choose runtime
        env_vars: Some(vec![
            ("ENV".to_string(), "production".to_string()),
            ("DEBUG".to_string(), "false".to_string())
        ]),
        working_dir: None,
        timeout_ms: Some(5000),
        memory_mb: Some(512),
        cpu_cores: Some(2),
        cache_key: Some("python-version-check".to_string()),
        snapshot_id: None,
        branch_from: None,
        mode: Some("cached".to_string()),
        payload: None,
    }).await?;

    println!("   Duration: {}ms", advanced_result.duration_ms);
    println!("   Output: {}", advanced_result.stdout.trim());

    // 7. Cached execution (should be faster second time)
    println!("\n6. Cached execution (second run):");
    let cached_result = client.run_cached(
        "echo 'This will be cached'",
        "alpine:latest"
    ).await?;
    println!("   Output: {}", cached_result);

    // 8. Check platform health
    println!("\n7. Platform health check:");
    let health = client.health_check().await?;
    println!("   Status: {}", health.status);
    println!("   Timestamp: {}", health.timestamp);

    // 9. Get client metrics
    println!("\n8. Client-side metrics:");
    let metrics = client.client_metrics().await;
    println!("   Total requests: {}", metrics.total_requests);
    println!("   Cache hit rate: {:.2}%", metrics.cache_hit_rate * 100.0);
    println!("   Average latency: {}ms", metrics.average_latency_ms);
    println!("   Error rate: {:.2}%", metrics.error_rate * 100.0);

    println!("\n✨ All advanced features demonstrated successfully!");

    Ok(())
}