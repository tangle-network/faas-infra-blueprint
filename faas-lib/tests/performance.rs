use faas_common::SandboxExecutor;
use faas_executor::docktopus::DockerBuilder;
use faas_executor::executor::{ContainerStrategy, ExecutionStrategy};
use faas_executor::{DockerExecutor, Executor, WarmContainer};
use faas_orchestrator::Orchestrator;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

// Helper to setup regular orchestrator
async fn setup_regular_orchestrator() -> color_eyre::Result<Arc<Orchestrator>> {
    let docker_builder = DockerBuilder::new().await?;
    let docker_client = docker_builder.client();
    let executor: Arc<dyn SandboxExecutor + Send + Sync> =
        Arc::new(DockerExecutor::new(docker_client));
    Ok(Arc::new(Orchestrator::new(executor)))
}

// Helper to setup performance test orchestrator
async fn setup_performance_orchestrator() -> color_eyre::Result<Arc<Orchestrator>> {
    let docker_builder = DockerBuilder::new().await?;
    let docker_client = docker_builder.client();

    let strategy = ExecutionStrategy::Container(ContainerStrategy {
        warm_pools: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        max_pool_size: 5,
        docker: docker_client,
        build_cache_volumes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        dependency_layers: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        gpu_pools: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
    });

    let executor: Arc<dyn SandboxExecutor + Send + Sync> = Arc::new(
        Executor::new(strategy)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to create executor: {}", e))?,
    );
    Ok(Arc::new(Orchestrator::new(executor)))
}

#[tokio::test]
async fn benchmark_execution_speed() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("=== EXECUTION SPEED BENCHMARK ===");

    // Setup both orchestrators
    let regular = setup_regular_orchestrator().await?;
    let performance_orchestrator = setup_performance_orchestrator().await?;

    let test_cases = vec![
        (
            "echo_test",
            vec!["echo".to_string(), "Benchmark test".to_string()],
        ),
        ("env_test", vec!["env".to_string()]),
        (
            "small_compile",
            vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo 'fn main() { println!(\"Hello\"); }' > main.rs && rustc main.rs && ./main"
                    .to_string(),
            ],
        ),
    ];

    for (test_name, command) in test_cases {
        info!("\n--- Testing: {} ---", test_name);

        // Test regular executor
        let regular_start = Instant::now();
        let regular_result = regular
            .schedule_execution(
                format!("regular-{}", test_name),
                if test_name == "small_compile" {
                    "rust:latest"
                } else {
                    "alpine:latest"
                }
                .to_string(),
                command.clone(),
                None,
                Vec::new(),
            )
            .await;
        let regular_duration = regular_start.elapsed();

        // Test performance_orchestrator executor
        let perf_start = Instant::now();
        let perf_result = performance_orchestrator
            .schedule_execution(
                format!("performance_orchestrator-{}", test_name),
                if test_name == "small_compile" {
                    "rust:latest"
                } else {
                    "alpine:latest"
                }
                .to_string(),
                command.clone(),
                None,
                Vec::new(),
            )
            .await;
        let perf_duration = perf_start.elapsed();

        // Report results
        match (regular_result, perf_result) {
            (Ok(_), Ok(_)) => {
                let speedup =
                    regular_duration.as_millis() as f64 / perf_duration.as_millis() as f64;
                info!("=== {} RESULTS ===", test_name.to_uppercase());
                info!(
                    "Regular: {:?} ({:.0}ms)",
                    regular_duration,
                    regular_duration.as_millis()
                );
                info!(
                    "Optimized: {:?} ({:.0}ms)",
                    perf_duration,
                    perf_duration.as_millis()
                );
                info!("Speedup: {:.2}x", speedup);

                // Check if we're hitting performance targets
                if test_name == "echo_test" && perf_duration.as_millis() < 250 {
                    info!("🚀 SUB-250MS TARGET ACHIEVED!");
                } else if perf_duration.as_millis() < 500 {
                    info!("✅ Good performance: {}ms", perf_duration.as_millis());
                } else {
                    warn!(
                        "⚠️ Still needs optimization: {}ms",
                        perf_duration.as_millis()
                    );
                }
            }
            (Err(e), _) => warn!("Regular executor failed: {}", e),
            (_, Err(e)) => warn!("Optimized executor failed: {}", e),
        }
    }

    Ok(())
}

#[tokio::test]
async fn target_250ms_cold_start() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("=== 250MS COLD START TARGET TEST ===");

    let performance_orchestrator = setup_performance_orchestrator().await?;

    // Multiple attempts to ensure consistency
    let mut attempts = Vec::new();
    for i in 0..5 {
        let start = Instant::now();

        let result = performance_orchestrator
            .schedule_execution(
                format!("cold-start-{}", i),
                "alpine:latest".to_string(),
                vec!["echo".to_string(), format!("Attempt {}", i)],
                None,
                Vec::new(),
            )
            .await;

        let duration = start.elapsed();
        attempts.push(duration);

        match result {
            Ok(_) => info!(
                "Attempt {}: {:?} ({:.0}ms)",
                i + 1,
                duration,
                duration.as_millis()
            ),
            Err(e) => warn!("Attempt {} failed: {}", i + 1, e),
        }
    }

    // Calculate statistics
    let successful_attempts: Vec<_> = attempts.iter().cloned().collect();
    if !successful_attempts.is_empty() {
        let avg_duration = successful_attempts.iter().sum::<std::time::Duration>()
            / successful_attempts.len() as u32;
        let min_duration = successful_attempts.iter().min().unwrap();
        let max_duration = successful_attempts.iter().max().unwrap();

        info!("=== COLD START STATISTICS ===");
        info!(
            "Average: {:?} ({:.0}ms)",
            avg_duration,
            avg_duration.as_millis()
        );
        info!(
            "Min: {:?} ({:.0}ms)",
            min_duration,
            min_duration.as_millis()
        );
        info!(
            "Max: {:?} ({:.0}ms)",
            max_duration,
            max_duration.as_millis()
        );

        // Performance assessment
        if avg_duration.as_millis() < 250 {
            info!(
                "🎯 TARGET ACHIEVED: Average {}ms < 250ms!",
                avg_duration.as_millis()
            );
        } else if avg_duration.as_millis() < 500 {
            info!("📈 CLOSE TO TARGET: Average {}ms", avg_duration.as_millis());
        } else {
            info!(
                "🔧 NEEDS OPTIMIZATION: Average {}ms",
                avg_duration.as_millis()
            );
        }

        // Consistency check
        let range = max_duration.as_millis() - min_duration.as_millis();
        if range < 100 {
            info!("✅ CONSISTENT PERFORMANCE: {}ms range", range);
        } else {
            info!("⚠️ VARIABLE PERFORMANCE: {}ms range", range);
        }
    }

    Ok(())
}

#[tokio::test]
async fn developer_workload_simulation() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("=== DEVELOPER WORKLOAD SIMULATION ===");

    let performance_orchestrator = setup_performance_orchestrator().await?;

    // Simulate typical development workflow
    let workflow_steps = vec![
        (
            "environment_setup",
            "alpine:latest",
            vec!["echo".to_string(), "Setting up environment".to_string()],
        ),
        (
            "dependency_install",
            "alpine:latest",
            vec![
                "apk".to_string(),
                "add".to_string(),
                "--no-cache".to_string(),
                "git".to_string(),
            ],
        ),
        (
            "code_analysis",
            "alpine:latest",
            vec![
                "find".to_string(),
                ".".to_string(),
                "-name".to_string(),
                "*.txt".to_string(),
            ],
        ),
        (
            "build_step",
            "alpine:latest",
            vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo 'Building...' && sleep 0.1 && echo 'Build complete'".to_string(),
            ],
        ),
    ];

    let workflow_start = Instant::now();
    let mut step_times = Vec::new();

    for (step_name, image, command) in workflow_steps {
        let step_start = Instant::now();

        match performance_orchestrator
            .schedule_execution(
                format!("workflow-{}", step_name),
                image.to_string(),
                command,
                None,
                Vec::new(),
            )
            .await
        {
            Ok(result) => {
                let step_duration = step_start.elapsed();
                step_times.push((step_name, step_duration));

                info!(
                    "Step '{}': {:?} ({:.0}ms)",
                    step_name,
                    step_duration,
                    step_duration.as_millis()
                );

                if let Some(response) = result.response {
                    let output = String::from_utf8_lossy(&response);
                    if !output.trim().is_empty() {
                        info!("  Output: {}", output.trim());
                    }
                }
            }
            Err(e) => warn!("Step '{}' failed: {}", step_name, e),
        }
    }

    let total_workflow_time = workflow_start.elapsed();

    info!("=== WORKFLOW SUMMARY ===");
    for (step_name, duration) in step_times {
        info!("{}: {:.0}ms", step_name, duration.as_millis());
    }
    info!(
        "Total workflow time: {:?} ({:.0}ms)",
        total_workflow_time,
        total_workflow_time.as_millis()
    );

    // Performance assessment for developer workflow
    if total_workflow_time.as_secs() < 5 {
        info!(
            "🚀 EXCELLENT: Complete workflow in {}s",
            total_workflow_time.as_secs()
        );
    } else if total_workflow_time.as_secs() < 10 {
        info!(
            "✅ GOOD: Complete workflow in {}s",
            total_workflow_time.as_secs()
        );
    } else {
        info!("⚠️ SLOW: Workflow took {}s", total_workflow_time.as_secs());
    }

    Ok(())
}
use faas_common::SandboxExecutor;
use faas_executor::docktopus::DockerBuilder;
use faas_executor::executor::{ContainerStrategy, ExecutionStrategy};
use faas_executor::{DockerExecutor, Executor, WarmContainer};
use faas_orchestrator::Orchestrator;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

// Helper to setup regular orchestrator
async fn setup_regular_orchestrator() -> color_eyre::Result<Arc<Orchestrator>> {
    let docker_builder = DockerBuilder::new().await?;
    let docker_client = docker_builder.client();
    let executor: Arc<dyn SandboxExecutor + Send + Sync> =
        Arc::new(DockerExecutor::new(docker_client));
    Ok(Arc::new(Orchestrator::new(executor)))
}

// Helper to setup fast orchestrator
async fn setup_fast_orchestrator() -> color_eyre::Result<Arc<Orchestrator>> {
    let docker_builder = DockerBuilder::new().await?;
    let docker_client = docker_builder.client();

    let strategy = ExecutionStrategy::Container(ContainerStrategy {
        warm_pools: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        max_pool_size: 5,
        docker: docker_client,
        build_cache_volumes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        dependency_layers: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        gpu_pools: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
    });

    let executor: Arc<dyn SandboxExecutor + Send + Sync> = Arc::new(
        Executor::new(strategy)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to create executor: {}", e))?,
    );
    Ok(Arc::new(Orchestrator::new(executor)))
}

#[tokio::test]
async fn ultrafast_simple_execution() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("=== ULTRAFAST vs REGULAR EXECUTION COMPARISON ===");

    // Warm up both executors
    info!("Warming up executors...");
    let regular_orchestrator = setup_regular_orchestrator().await?;
    let fast_orchestrator = setup_fast_orchestrator().await?;

    // Give the fast orchestrator time to pre-warm containers
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let test_cases = vec![
        (
            "echo",
            vec!["echo".to_string(), "Hello Fast FaaS!".to_string()],
        ),
        ("date", vec!["date".to_string()]),
        ("uname", vec!["uname".to_string(), "-a".to_string()]),
    ];

    for (test_name, command) in test_cases {
        info!("\n--- Testing: {} ---", test_name);

        // Test regular executor (3 runs for average)
        let mut regular_times = Vec::new();
        for i in 0..3 {
            let start = Instant::now();

            match regular_orchestrator
                .schedule_execution(
                    format!("regular-{}-{}", test_name, i),
                    "alpine:latest".to_string(),
                    command.clone(),
                    None,
                    Vec::new(),
                )
                .await
            {
                Ok(result) => {
                    let duration = start.elapsed();
                    regular_times.push(duration);
                    info!("Regular run {}: {:?}", i + 1, duration);

                    if let Some(response) = result.response {
                        info!(
                            "Regular output: {}",
                            String::from_utf8_lossy(&response).trim()
                        );
                    }
                }
                Err(e) => warn!("Regular execution failed: {}", e),
            }
        }

        // Test fast executor (3 runs for average)
        let mut fast_times = Vec::new();
        for i in 0..3 {
            let start = Instant::now();

            match fast_orchestrator
                .schedule_execution(
                    format!("fast-{}-{}", test_name, i),
                    "alpine:latest".to_string(),
                    command.clone(),
                    None,
                    Vec::new(),
                )
                .await
            {
                Ok(result) => {
                    let duration = start.elapsed();
                    fast_times.push(duration);
                    info!("Fast run {}: {:?}", i + 1, duration);

                    if let Some(response) = result.response {
                        info!("Fast output: {}", String::from_utf8_lossy(&response).trim());
                    }
                }
                Err(e) => warn!("Fast execution failed: {}", e),
            }
        }

        // Calculate averages
        if !regular_times.is_empty() && !fast_times.is_empty() {
            let regular_avg =
                regular_times.iter().sum::<std::time::Duration>() / regular_times.len() as u32;
            let fast_avg = fast_times.iter().sum::<std::time::Duration>() / fast_times.len() as u32;

            let speedup = regular_avg.as_millis() as f64 / fast_avg.as_millis() as f64;

            info!("=== {} RESULTS ===", test_name.to_uppercase());
            info!(
                "Regular average: {:?} ({:.0}ms)",
                regular_avg,
                regular_avg.as_millis()
            );
            info!(
                "Fast average: {:?} ({:.0}ms)",
                fast_avg,
                fast_avg.as_millis()
            );
            info!("SPEEDUP: {:.2}x", speedup);

            if speedup >= 2.0 {
                info!("🚀 FAST EXECUTOR IS {}x FASTER!", speedup);
            } else if speedup >= 1.1 {
                info!("✅ Fast executor is faster by {:.2}x", speedup);
            } else {
                warn!("⚠️ Fast executor not significantly faster: {:.2}x", speedup);
            }
        }
    }

    Ok(())
}

#[tokio::test]
async fn ultrafast_burst_test() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("=== BURST EXECUTION TEST ===");

    let fast_orchestrator = setup_fast_orchestrator().await?;

    // Give time for container pre-warming
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let num_concurrent = 10;
    let start = Instant::now();

    // Launch many concurrent executions
    let mut handles = Vec::new();
    for i in 0..num_concurrent {
        let orch = fast_orchestrator.clone();
        let handle = tokio::spawn(async move {
            let task_start = Instant::now();
            let result = orch
                .schedule_execution(
                    format!("burst-{}", i),
                    "alpine:latest".to_string(),
                    vec!["echo".to_string(), format!("Burst task {}", i)],
                    None,
                    Vec::new(),
                )
                .await;
            let task_duration = task_start.elapsed();
            (i, result, task_duration)
        });
        handles.push(handle);
    }

    // Wait for all to complete
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await?);
    }

    let total_duration = start.elapsed();

    info!("=== BURST TEST RESULTS ===");
    info!(
        "Total time for {} concurrent executions: {:?}",
        num_concurrent, total_duration
    );

    let successful = results
        .iter()
        .filter(|(_, result, _)| result.is_ok())
        .count();
    let failed = results.len() - successful;

    info!("Successful: {}, Failed: {}", successful, failed);

    if successful > 0 {
        let task_times: Vec<_> = results
            .iter()
            .filter(|(_, result, _)| result.is_ok())
            .map(|(_, _, duration)| *duration)
            .collect();

        let avg_task_time =
            task_times.iter().sum::<std::time::Duration>() / task_times.len() as u32;
        let min_task_time = task_times.iter().min().unwrap();
        let max_task_time = task_times.iter().max().unwrap();

        info!(
            "Average task time: {:?} ({:.0}ms)",
            avg_task_time,
            avg_task_time.as_millis()
        );
        info!(
            "Min task time: {:?} ({:.0}ms)",
            min_task_time,
            min_task_time.as_millis()
        );
        info!(
            "Max task time: {:?} ({:.0}ms)",
            max_task_time,
            max_task_time.as_millis()
        );

        // Test if we achieved ultra-fast execution (target <100ms average)
        if avg_task_time.as_millis() < 100 {
            info!(
                "🚀 ULTRA-FAST ACHIEVED: Average {}ms < 100ms target!",
                avg_task_time.as_millis()
            );
        } else if avg_task_time.as_millis() < 200 {
            info!("✅ FAST: Average {}ms < 200ms", avg_task_time.as_millis());
        } else {
            warn!("⚠️ Still slow: Average {}ms", avg_task_time.as_millis());
        }
    }

    Ok(())
}

#[tokio::test]
async fn cold_vs_warm_start_comparison() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("=== COLD vs WARM START COMPARISON ===");

    let fast_orchestrator = setup_fast_orchestrator().await?;

    // Cold start test (before pool warms up)
    info!("Testing cold start...");
    let cold_start = Instant::now();
    let cold_result = fast_orchestrator
        .schedule_execution(
            "cold-start".to_string(),
            "alpine:latest".to_string(),
            vec!["echo".to_string(), "Cold start".to_string()],
            None,
            Vec::new(),
        )
        .await;
    let cold_duration = cold_start.elapsed();

    // Give time for pool to warm up
    info!("Warming up container pool...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Warm start test (after pool is ready)
    info!("Testing warm start...");
    let warm_start = Instant::now();
    let warm_result = fast_orchestrator
        .schedule_execution(
            "warm-start".to_string(),
            "alpine:latest".to_string(),
            vec!["echo".to_string(), "Warm start".to_string()],
            None,
            Vec::new(),
        )
        .await;
    let warm_duration = warm_start.elapsed();

    info!("=== START COMPARISON RESULTS ===");

    match &cold_result {
        Ok(_) => info!(
            "Cold start: {:?} ({:.0}ms)",
            cold_duration,
            cold_duration.as_millis()
        ),
        Err(e) => warn!("Cold start failed: {}", e),
    }

    match &warm_result {
        Ok(_) => info!(
            "Warm start: {:?} ({:.0}ms)",
            warm_duration,
            warm_duration.as_millis()
        ),
        Err(e) => warn!("Warm start failed: {}", e),
    }

    if cold_result.is_ok() && warm_result.is_ok() {
        let speedup = cold_duration.as_millis() as f64 / warm_duration.as_millis() as f64;
        info!("Warm start speedup: {:.2}x", speedup);

        if warm_duration.as_millis() < 50 {
            info!(
                "🚀 LIGHTNING FAST: {}ms warm start!",
                warm_duration.as_millis()
            );
        } else if warm_duration.as_millis() < 100 {
            info!("⚡ ULTRA-FAST: {}ms warm start!", warm_duration.as_millis());
        }
    }

    Ok(())
}
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
    let executor: Arc<dyn SandboxExecutor + Send + Sync> =
        Arc::new(DockerExecutor::new(docker_client));
    Ok(Arc::new(Orchestrator::new(executor)))
}

#[tokio::test]
async fn benchmark_simple_execution() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let orchestrator = setup_orchestrator().await?;

    info!("=== Simple Echo Benchmark ===");
    let start = Instant::now();

    let result = orchestrator
        .schedule_execution(
            "echo-test".to_string(),
            "alpine:latest".to_string(),
            vec!["echo".to_string(), "Hello FaaS!".to_string()],
            None,
            b"test payload".to_vec(),
        )
        .await?;

    let duration = start.elapsed();
    info!("Simple echo completed in: {:?}", duration);
    info!(
        "Result: {:?}",
        String::from_utf8_lossy(&result.response.unwrap_or_default())
    );

    assert!(result.error.is_none());
    assert!(
        duration.as_secs() < 10,
        "Simple echo took too long: {:?}",
        duration
    );

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

    let result = orchestrator
        .schedule_execution(
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
        )
        .await?;

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

    let result = orchestrator
        .schedule_execution(
            "python-data".to_string(),
            "python:3.11-alpine".to_string(),
            vec![
                "python".to_string(),
                "-c".to_string(),
                python_program.to_string(),
            ],
            None,
            json_data.as_bytes().to_vec(),
        )
        .await?;

    let duration = start.elapsed();
    info!("Python processing completed in: {:?}", duration);

    if let Some(response) = &result.response {
        info!("Python output: {}", String::from_utf8_lossy(response));
    }

    assert!(
        result.error.is_none(),
        "Python processing failed: {:?}",
        result.error
    );
    assert!(
        duration.as_secs() < 30,
        "Python processing took too long: {:?}",
        duration
    );

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
            let result = orch
                .schedule_execution(
                    format!("concurrent-{}", i),
                    "alpine:latest".to_string(),
                    vec![
                        "sh".to_string(),
                        "-c".to_string(),
                        format!("echo 'Task {}' && sleep 1", i),
                    ],
                    None,
                    Vec::new(),
                )
                .await;
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
    assert!(
        total_duration.as_secs() < 10,
        "Concurrent execution too slow: {:?}",
        total_duration
    );

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

    let result = orchestrator
        .schedule_execution(
            "large-payload".to_string(),
            "alpine:latest".to_string(),
            vec![
                "sh".to_string(),
                "-c".to_string(),
                "wc -c && echo 'Processed large payload'".to_string(),
            ],
            None,
            large_payload,
        )
        .await?;

    let duration = start.elapsed();
    info!("Large payload processing completed in: {:?}", duration);

    if let Some(response) = &result.response {
        info!(
            "Large payload output: {}",
            String::from_utf8_lossy(response)
        );
    }

    assert!(result.error.is_none());
    assert!(
        duration.as_secs() < 15,
        "Large payload took too long: {:?}",
        duration
    );

    Ok(())
}
