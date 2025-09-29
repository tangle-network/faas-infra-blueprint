//! FaaS Platform Rust Examples
//!
//! Demonstrates using the platform from Rust applications.

use faas_client_sdk::{FaasClient, Runtime, ExecutionMode, ExecuteRequest, AdvancedExecuteRequest, CreateSnapshotRequest};
use std::time::Duration;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 FaaS Platform Rust Examples\n");

    // Initialize client
    let client = FaasClient::new("http://localhost:8080".to_string());

    // Example 1: Simple command execution
    println!("1. Simple command execution:");
    let result = client.execute(ExecuteRequest {
        command: "echo 'Hello from Rust!'".to_string(),
        image: Some("alpine:latest".to_string()),
        runtime: None,
        env_vars: None,
        working_dir: None,
        timeout_ms: Some(5000),
        cache_key: None,
    }).await?;

    println!("   Output: {}", result.stdout);
    println!("   Duration: {}ms\n", result.duration_ms);

    // Example 2: Running Python code
    println!("2. Running Python code:");
    let python_code = r#"
import sys
import platform

print(f"Python {sys.version}")
print(f"Platform: {platform.platform()}")
print("Hello from Python via Rust!")
"#;

    let result = client.execute(ExecuteRequest {
        command: format!("python -c '{}'", python_code.replace('\'', "\\'")),
        image: Some("python:3.11-slim".to_string()),
        env_vars: None,
        working_dir: None,
        timeout_ms: Some(10000),
    }).await?;

    println!("   Output:\n{}\n", result.stdout);

    // Example 3: Using environment variables
    println!("3. Using environment variables:");
    let result = client.execute(ExecuteRequest {
        command: "echo $MESSAGE && echo $VERSION".to_string(),
        image: Some("alpine:latest".to_string()),
        env_vars: Some(vec![
            ("MESSAGE".to_string(), "Hello from Rust!".to_string()),
            ("VERSION".to_string(), "1.0.0".to_string()),
        ]),
        working_dir: None,
        timeout_ms: Some(5000),
    }).await?;

    println!("   Output: {}\n", result.stdout);

    // Example 4: Advanced execution with mode
    println!("4. Advanced execution with caching:");
    let advanced_result = client.execute_advanced(faas_sdk::AdvancedExecuteRequest {
        command: "date +%s && echo 'Cached result'".to_string(),
        image: "alpine:latest".to_string(),
        mode: ExecutionMode::Cached,
        env_vars: None,
        memory_mb: Some(256),
        cpu_cores: Some(1),
        use_snapshots: Some(true),
    }).await?;

    println!("   First run output: {}", advanced_result.stdout);
    println!("   Duration: {}ms", advanced_result.duration_ms);

    // Run again to test caching
    let cached_result = client.execute_advanced(faas_sdk::AdvancedExecuteRequest {
        command: "date +%s && echo 'Cached result'".to_string(),
        image: "alpine:latest".to_string(),
        mode: ExecutionMode::Cached,
        env_vars: None,
        memory_mb: Some(256),
        cpu_cores: Some(1),
        use_snapshots: Some(true),
    }).await?;

    println!("   Second run output: {}", cached_result.stdout);
    println!("   Duration: {}ms", cached_result.duration_ms);

    if cached_result.duration_ms < advanced_result.duration_ms / 10 {
        println!("   ✅ Caching worked! Second run was much faster\n");
    }

    // Example 5: Creating a snapshot
    println!("5. Creating and using snapshots:");

    // First, create a container with some state
    let setup_result = client.execute(ExecuteRequest {
        command: "echo 'Initial state' > /tmp/state.txt && cat /tmp/state.txt".to_string(),
        image: Some("alpine:latest".to_string()),
        env_vars: None,
        working_dir: None,
        timeout_ms: Some(5000),
    }).await?;

    println!("   Initial state: {}", setup_result.stdout);

    // Create snapshot (if the container is still running)
    match client.create_snapshot(faas_sdk::CreateSnapshotRequest {
        name: "rust-example-snapshot".to_string(),
        container_id: setup_result.request_id.clone(),
        description: Some("Snapshot from Rust example".to_string()),
    }).await {
        Ok(snapshot) => {
            println!("   Created snapshot: {}", snapshot.snapshot_id);
            println!("   Size: {} bytes\n", snapshot.size_bytes);
        }
        Err(e) => {
            println!("   Snapshot creation not available: {}\n", e);
        }
    }

    // Example 6: Performance metrics
    println!("6. Getting performance metrics:");
    let metrics = client.get_metrics().await?;
    println!("   Average execution time: {:.2}ms", metrics.avg_execution_time_ms);
    println!("   Cache hit rate: {:.2}%", metrics.cache_hit_rate * 100.0);
    println!("   Active containers: {}", metrics.active_containers);
    println!("   Memory usage: {}MB\n", metrics.memory_usage_mb);

    // Example 7: Health check
    println!("7. Platform health check:");
    let health = client.health().await?;
    println!("   Status: {}", health.status);
    if let Some(components) = health.components {
        println!("   Components:");
        for (name, status) in components {
            println!("     - {}: {}", name, status);
        }
    }

    // Example 8: Rust-specific workload
    println!("\n8. Compiling and running Rust code:");
    let rust_code = r#"
fn main() {
    println!("Hello from compiled Rust!");

    let numbers = vec![1, 2, 3, 4, 5];
    let sum: i32 = numbers.iter().sum();
    println!("Sum of {:?} = {}", numbers, sum);

    // Demonstrate Rust's memory safety
    let message = String::from("Rust is memory safe!");
    println!("{}", message);
}
"#;

    let compile_and_run = format!(
        "echo '{}' > main.rs && rustc main.rs -o main && ./main",
        rust_code.replace('\'', "\\'").replace('"', "\\\"")
    );

    let rust_result = client.execute(ExecuteRequest {
        command: compile_and_run,
        image: Some("rust:latest".to_string()),
        env_vars: None,
        working_dir: None,
        timeout_ms: Some(30000), // Compilation takes time
    }).await?;

    println!("   Rust compilation output:\n{}", rust_result.stdout);

    // Example 9: Concurrent executions
    println!("\n9. Concurrent executions:");
    use futures::future::join_all;

    let tasks: Vec<_> = (0..5)
        .map(|i| {
            let client = client.clone();
            tokio::spawn(async move {
                let result = client.execute(ExecuteRequest {
                    command: format!("echo 'Task {} completed'", i),
                    image: Some("alpine:latest".to_string()),
                    env_vars: None,
                    working_dir: None,
                    timeout_ms: Some(5000),
                }).await;
                (i, result)
            })
        })
        .collect();

    let results = join_all(tasks).await;
    for result in results {
        match result {
            Ok((i, Ok(exec_result))) => {
                println!("   Task {}: {}", i, exec_result.stdout.trim());
            }
            _ => println!("   Task failed"),
        }
    }

    println!("\n✅ All Rust examples completed successfully!");

    Ok(())
}