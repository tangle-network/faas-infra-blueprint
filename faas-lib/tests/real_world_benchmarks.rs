use faas_common::SandboxExecutor;
use faas_executor::docktopus::DockerBuilder;
use faas_executor::DockerExecutor;
use faas_orchestrator::Orchestrator;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

// Helper to setup orchestrator
async fn setup_orchestrator() -> color_eyre::Result<Arc<Orchestrator>> {
    let docker_builder = DockerBuilder::new().await?;
    let docker_client = docker_builder.client();
    let executor: Arc<dyn SandboxExecutor + Send + Sync> = Arc::new(DockerExecutor::new(docker_client));
    Ok(Arc::new(Orchestrator::new(executor)))
}

#[tokio::test]
async fn benchmark_simple_execution() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let orchestrator = setup_orchestrator().await?;

    info!("=== Simple Echo Benchmark ===");
    let start = Instant::now();

    let result = orchestrator.schedule_execution(
        "echo-test".to_string(),
        "alpine:latest".to_string(),
        vec!["echo".to_string(), "Hello FaaS!".to_string()],
        None,
        b"test payload".to_vec(),
    ).await?;

    let duration = start.elapsed();
    info!("Simple echo completed in: {:?}", duration);
    info!("Result: {:?}", String::from_utf8_lossy(&result.response.unwrap_or_default()));

    assert!(result.error.is_none());
    assert!(duration.as_secs() < 10, "Simple echo took too long: {:?}", duration);

    Ok(())
}

#[tokio::test]
async fn benchmark_rust_compilation() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let orchestrator = setup_orchestrator().await?;

    info!("=== Rust Compilation Benchmark ===");

    // Create a simple Rust program
    let rust_program = r#"
fn main() {
    println!("Hello from compiled Rust!");
    println!("Computing fibonacci(30):");
    fn fib(n: u32) -> u32 {
        if n <= 1 { n } else { fib(n-1) + fib(n-2) }
    }
    println!("Result: {}", fib(30));
}
"#;

    let start = Instant::now();

    let result = orchestrator.schedule_execution(
        "rust-compile".to_string(),
"rust:latest".to_string(),
        vec![
            "sh".to_string(),
            "-c".to_string(),
            format!(
                "echo '{}' > main.rs && rustc main.rs && ./main",
                rust_program.replace('\n', "\\n").replace('\'', "\\'")
            ),
        ],
        None,
        Vec::new(),
    ).await?;

    let duration = start.elapsed();
    info!("Rust compilation completed in: {:?}", duration);

    if let Some(ref error) = result.error {
        warn!("Rust compilation error: {}", error);
    }

    if let Some(response) = &result.response {
        info!("Rust output: {}", String::from_utf8_lossy(response));
    }

    if let Some(logs) = &result.logs {
        info!("Rust logs: {}", logs);
    }

    // Don't fail test if Rust image is slow to pull, but report timing
    info!("Rust compilation benchmark: {}ms", duration.as_millis());

    Ok(())
}

#[tokio::test]
async fn benchmark_python_data_processing() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let orchestrator = setup_orchestrator().await?;

    info!("=== Python Data Processing Benchmark ===");

    let python_program = r#"
import json
import sys
import time

# Read JSON from stdin
data = json.loads(input())
print(f"Processing {len(data)} items...")

# Simulate data processing
start = time.time()
result = sum(x * 2 for x in data if x % 2 == 0)
end = time.time()

print(f"Result: {result}")
print(f"Processing took: {end - start:.3f}s")
print(json.dumps({"processed_result": result, "processing_time": end - start}))
"#;

    let test_data = (1..=1000).collect::<Vec<i32>>();
    let json_data = serde_json::to_string(&test_data)?;

    let start = Instant::now();

    let result = orchestrator.schedule_execution(
        "python-data".to_string(),
        "python:3.11-alpine".to_string(),
        vec![
            "python".to_string(),
            "-c".to_string(),
            python_program.to_string(),
        ],
        None,
        json_data.as_bytes().to_vec(),
    ).await?;

    let duration = start.elapsed();
    info!("Python processing completed in: {:?}", duration);

    if let Some(response) = &result.response {
        info!("Python output: {}", String::from_utf8_lossy(response));
    }

    assert!(result.error.is_none(), "Python processing failed: {:?}", result.error);
    assert!(duration.as_secs() < 30, "Python processing took too long: {:?}", duration);

    Ok(())
}

#[tokio::test]
async fn benchmark_concurrent_executions() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let orchestrator = setup_orchestrator().await?;

    info!("=== Concurrent Executions Benchmark ===");

    let start = Instant::now();

    // Launch 5 concurrent simple tasks
    let mut handles = Vec::new();
    for i in 0..5 {
        let orch = orchestrator.clone();
        let handle = tokio::spawn(async move {
            let task_start = Instant::now();
            let result = orch.schedule_execution(
                format!("concurrent-{}", i),
                "alpine:latest".to_string(),
                vec!["sh".to_string(), "-c".to_string(), format!("echo 'Task {}' && sleep 1", i)],
                None,
                Vec::new(),
            ).await;
            let task_duration = task_start.elapsed();
            (i, result, task_duration)
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await?);
    }

    let total_duration = start.elapsed();
    info!("All concurrent tasks completed in: {:?}", total_duration);

    for (i, result, task_duration) in results {
        match result {
            Ok(res) => {
                info!("Task {} completed in {:?}", i, task_duration);
                if let Some(response) = res.response {
                    info!("Task {} output: {}", i, String::from_utf8_lossy(&response));
                }
            }
            Err(e) => warn!("Task {} failed: {:?}", i, e),
        }
    }

    // Concurrent tasks should finish faster than sequential
    assert!(total_duration.as_secs() < 10, "Concurrent execution too slow: {:?}", total_duration);

    Ok(())
}

#[tokio::test]
async fn benchmark_large_payload() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let orchestrator = setup_orchestrator().await?;

    info!("=== Large Payload Benchmark ===");

    // Create 1MB payload
    let large_payload = vec![b'A'; 1024 * 1024];

    let start = Instant::now();

    let result = orchestrator.schedule_execution(
        "large-payload".to_string(),
        "alpine:latest".to_string(),
        vec![
            "sh".to_string(),
            "-c".to_string(),
            "wc -c && echo 'Processed large payload'".to_string(),
        ],
        None,
        large_payload,
    ).await?;

    let duration = start.elapsed();
    info!("Large payload processing completed in: {:?}", duration);

    if let Some(response) = &result.response {
        info!("Large payload output: {}", String::from_utf8_lossy(response));
    }

    assert!(result.error.is_none());
    assert!(duration.as_secs() < 15, "Large payload took too long: {:?}", duration);

    Ok(())
}