//! Complete FaaS SDK Demo
//!
//! Demonstrates all SDK features:
//! - Basic execution
//! - Advanced execution with caching
//! - Snapshot management
//! - Instance lifecycle
//! - Metrics monitoring

use faas_client_sdk::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = FaasClient::new("http://localhost:8080".to_string());

    println!("🚀 FaaS Platform Demo");

    // 1. Health Check
    println!("\n1. Checking platform health...");
    let health = client.health_check().await?
    println!("Status: {} ({})", health.status, health.timestamp);

    // 2. Basic Execution
    println!("\n2. Basic function execution...");
    let basic_result = client.run("echo 'Hello from FaaS!'").await?;
    println!("Output: {}", basic_result.trim());

    // 3. Custom Image Execution
    println!("\n3. Custom image execution...");
    let python_result = client.run_with_image("python3 -c 'print(2**10)'", "python:3.9-alpine").await?;
    println!("Python result: {}", python_result.trim());

    // 4. Advanced Execution with Caching
    println!("\n4. Advanced execution with caching...");
    let cached_result = client.run_cached("ls -la /usr", "alpine:latest").await?;
    println!("Cached execution completed");

    // 5. Advanced Execution Request
    println!("\n5. Advanced execution with custom configuration...");
    let advanced_request = AdvancedExecuteRequest {
        command: "free -m".to_string(),
        image: "alpine:latest".to_string(),
        mode: ExecutionMode::Cached,
        env_vars: Some(vec![("MY_VAR".to_string(), "test_value".to_string())]),
        branch_from: None,
        checkpoint_id: None,
        enable_gpu: false,
        gpu_model: None,
        memory_mb: Some(512),
        cpu_cores: Some(1),
        use_snapshots: Some(true),
    };

    let advanced_result = client.execute_advanced(advanced_request).await?;
    println!("Memory info: {}", advanced_result.stdout.lines().take(3).collect::<Vec<_>>().join("\n"));

    // 6. Create Development Environment
    println!("\n6. Creating persistent development environment...");
    let dev_env_id = client.create_dev_env("my-rust-env", "rust:1.75").await?;
    println!("Created development environment: {}", dev_env_id);

    // 7. List Instances
    println!("\n7. Listing active instances...");
    let instances = client.list_instances().await?;
    println!("Active instances: {}", instances.len());
    for instance in &instances {
        println!("  - {} ({})", instance.instance_id, instance.status);
    }

    // 8. Create and Manage Snapshots
    println!("\n8. Snapshot management...");

    // First, we need a container to snapshot (simplified for demo)
    let container_id = "demo_container_123"; // In reality, get from execution result

    let snapshot_request = CreateSnapshotRequest {
        name: "demo-snapshot".to_string(),
        container_id: container_id.to_string(),
        description: Some("Demo snapshot for testing".to_string()),
    };

    // Note: This will fail in demo since we don't have a real container
    match client.create_snapshot(snapshot_request).await {
        Ok(snapshot) => {
            println!("Created snapshot: {} ({})", snapshot.snapshot_id, snapshot.name);

            // List all snapshots
            let snapshots = client.list_snapshots().await?;
            println!("Total snapshots: {}", snapshots.len());
        }
        Err(e) => {
            println!("Snapshot creation failed (expected in demo): {}", e);
        }
    }

    // 9. Performance Metrics
    println!("\n9. Performance metrics...");
    let metrics = client.get_metrics().await?;
    println!("Average execution time: {}ms", metrics.avg_execution_time_ms);
    println!("Cache hit rate: {:.1}%", metrics.cache_hit_rate * 100.0);
    println!("Active containers: {}", metrics.active_containers);
    println!("Memory usage: {}MB", metrics.memory_usage_mb);
    println!("CPU usage: {:.1}%", metrics.cpu_usage_percent);

    // 10. Cleanup
    println!("\n10. Cleanup...");
    for instance in instances {
        match client.stop_instance(&instance.instance_id).await {
            Ok(_) => println!("Stopped instance: {}", instance.instance_id),
            Err(e) => println!("Failed to stop {}: {}", instance.instance_id, e),
        }
    }

    println!("\n✅ Demo completed successfully!");
    Ok(())
}