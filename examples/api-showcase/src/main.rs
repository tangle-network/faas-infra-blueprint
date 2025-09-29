//! Complete showcase of building advanced services on FaaS platform
//!
//! Demonstrates how to build complex, production-ready services
//! using the actual FaaS platform APIs.

use faas_client_sdk::{
    FaasClient, ExecuteRequest, Runtime, ForkBranch,
};
use std::time::Instant;

/// Example: ML Pipeline with real execution
async fn ml_pipeline_example(client: &FaasClient) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📊 ML Pipeline Example\n");

    // Step 1: Data preparation
    let data_prep = client.run_python(r#"
import json
import random

# Generate synthetic data
data = {
    "features": [[random.random() for _ in range(10)] for _ in range(100)],
    "labels": [random.randint(0, 1) for _ in range(100)]
}
print(json.dumps({"status": "prepared", "samples": len(data["features"])}))
"#).await?;
    println!("Data prep: {}", data_prep.stdout);

    // Step 2: Feature engineering (parallel A/B test)
    let _feature_branches = vec![
        ForkBranch {
            id: "standard-scaling".to_string(),
            command: r#"python -c "print('Standard scaling applied')"#.to_string(),
            env_vars: None,
            weight: Some(0.5),
        },
        ForkBranch {
            id: "min-max-scaling".to_string(),
            command: r#"python -c "print('Min-max scaling applied')"#.to_string(),
            env_vars: None,
            weight: Some(0.5),
        },
    ];

    // Create base execution first, then fork from it
    let base = client.run_python("print('Base model initialized')").await?;

    let standard_scaling = client.fork_execution(&base.request_id, r#"python -c "print('Standard scaling applied')"#).await?;
    let minmax_scaling = client.fork_execution(&base.request_id, r#"python -c "print('Min-max scaling applied')"#).await?;

    println!("Scaling options tested:");
    println!("  Standard: {}", standard_scaling.stdout.trim());
    println!("  Min-max: {}", minmax_scaling.stdout.trim());

    // Step 3: Model training
    let training = client.run_python(r#"
import json
import time

# Simulate model training
start = time.time()
time.sleep(0.1)  # Simulate training
accuracy = 0.92 + (time.time() % 0.08)

result = {
    "model": "random_forest",
    "accuracy": accuracy,
    "training_time": time.time() - start
}
print(json.dumps(result))
"#).await?;
    println!("Training result: {}", training.stdout);

    Ok(())
}

/// Example: Real-time API with caching
async fn api_service_example(client: &FaasClient) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🌐 Real-time API Service\n");

    // Simulate API endpoints with caching
    let endpoints = vec![
        ("user-data", "SELECT * FROM users WHERE id = 1"),
        ("product-catalog", "SELECT * FROM products LIMIT 10"),
        ("analytics", "SELECT COUNT(*) FROM events"),
    ];

    for (endpoint, query) in endpoints {
        let start = Instant::now();

        // Use caching for repeated queries
        let result = client.execute(ExecuteRequest {
            command: format!("echo 'Executing: {}'", query),
            image: Some("alpine:latest".to_string()),
            runtime: Some(Runtime::Docker),
            env_vars: None,
            working_dir: None,
            timeout_ms: Some(1000),
            cache_key: Some(format!("api-{}", endpoint)),
        }).await?;

        println!("/{}: {} ({}ms, cached: {})",
                 endpoint,
                 result.stdout.trim(),
                 start.elapsed().as_millis(),
                 false); // cached flag not available
    }

    Ok(())
}

/// Example: Serverless workflow orchestration
async fn workflow_example(client: &FaasClient) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⚡ Serverless Workflow\n");

    // Step 1: Input validation
    let validation = client.run_javascript(r#"
const input = { email: "user@example.com", amount: 100 };
const valid = input.email.includes('@') && input.amount > 0;
console.log(JSON.stringify({ valid, input }));
"#).await?;
    println!("Validation: {}", validation.stdout.trim());

    // Step 2: Simulate business logic processing
    let base_execution = client.execute(ExecuteRequest {
        command: "echo 'base execution completed'".to_string(),
        image: Some("alpine:latest".to_string()),
        runtime: None,
        env_vars: None,
        working_dir: None,
        timeout_ms: None,
        cache_key: None,
    }).await?;

    let payment_fork = client.fork_execution(&base_execution.request_id, "echo 'payment processed'").await?;
    let inventory_fork = client.fork_execution(&base_execution.request_id, "echo 'inventory updated'").await?;
    let notification_fork = client.fork_execution(&base_execution.request_id, "echo 'notification sent'").await?;

    println!("Workflow completed with parallel processing:");
    println!("  Payment: {}", payment_fork.stdout.trim());
    println!("  Inventory: {}", inventory_fork.stdout.trim());
    println!("  Notification: {}", notification_fork.stdout.trim());

    // Step 3: Result aggregation
    let aggregation = client.run_python(r#"
import json
results = [
    {"payment": "processed"},
    {"inventory": "updated"},
    {"notification": "sent"}
]
summary = {
    "status": "success",
    "completed_tasks": len(results),
    "timestamp": "2024-01-01T12:00:00Z"
}
print(json.dumps(summary))
"#).await?;
    println!("Aggregation: {}", aggregation.stdout.trim());

    Ok(())
}

/// Example: Container prewarming for low latency
async fn performance_example(client: &FaasClient) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🚀 Performance Optimization\n");

    // Prewarm containers for different runtimes
    let runtimes = vec![
        (Runtime::Docker, "alpine:latest", 3),
        (Runtime::Firecracker, "alpine:latest", 2),
    ];

    for (runtime, image, count) in runtimes {
        client.prewarm(image, count).await?;

        println!("Prewarmed {} {} containers",
                 count,
                 match runtime {
                     Runtime::Docker => "Docker",
                     Runtime::Firecracker => "Firecracker",
                     Runtime::Auto => "Auto",
                 });
    }

    // Test warm start performance
    println!("\nWarm start performance test:");
    for i in 0..3 {
        let start = Instant::now();
        let _result = client.execute(ExecuteRequest {
            command: format!("echo 'Warm start test {}'", i),
            image: Some("alpine:latest".to_string()),
            runtime: Some(Runtime::Docker),
            env_vars: None,
            working_dir: None,
            timeout_ms: Some(1000),
            cache_key: Some("warm-start-test".to_string()),
        }).await?;

        println!("  Run {}: {}ms (cached: {})",
                 i + 1,
                 start.elapsed().as_millis(),
                 false); // cached flag not available
    }

    Ok(())
}

/// Example: Platform metrics and monitoring
async fn monitoring_example(client: &FaasClient) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📈 Platform Monitoring\n");

    // Get platform metrics
    let metrics = client.get_metrics().await?;

    println!("Platform Metrics:");
    println!("  Total executions: {}", metrics.total_executions);
    println!("  Avg execution time: {:.2}ms", metrics.avg_execution_time_ms);
    println!("  Cache hit rate: {:.1}%", metrics.cache_hit_rate * 100.0);
    println!("  Active containers: {}", metrics.active_containers);
    println!("  Memory usage: {}MB", metrics.memory_usage_mb);

    // Check platform health
    let health = client.health().await?;
    println!("\nHealth Status: {}", health.status);

    if let Some(components) = health.components {
        println!("Component Health:");
        for (component, status) in components {
            println!("  {}: {}", component, status);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 FaaS Platform Advanced Showcase\n");
    println!("This demonstrates building production services using the platform.\n");

    let client = FaasClient::new("http://localhost:8080".to_string());

    // Run all examples
    ml_pipeline_example(&client).await?;
    api_service_example(&client).await?;
    workflow_example(&client).await?;
    performance_example(&client).await?;
    monitoring_example(&client).await?;

    println!("\n✅ All examples completed successfully!");

    Ok(())
}