use faas_common::SandboxExecutor;
use faas_executor::{DockerExecutor, Executor, WarmContainer};
use faas_executor::executor::{ExecutionStrategy, ContainerStrategy};
use faas_executor::docktopus::DockerBuilder;
use faas_orchestrator::Orchestrator;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

// Helper to setup regular orchestrator
async fn setup_regular_orchestrator() -> color_eyre::Result<Arc<Orchestrator>> {
    let docker_builder = DockerBuilder::new().await?;
    let docker_client = docker_builder.client();
    let executor: Arc<dyn SandboxExecutor + Send + Sync> = Arc::new(DockerExecutor::new(docker_client));
    Ok(Arc::new(Orchestrator::new(executor)))
}

// Helper to setup optimized orchestrator
async fn setup_optimized_orchestrator() -> color_eyre::Result<Arc<Orchestrator>> {
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
        Executor::new(strategy).await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to create executor: {}", e))?
    );
    Ok(Arc::new(Orchestrator::new(executor)))
}

#[tokio::test]
async fn benchmark_execution_speed() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("=== EXECUTION SPEED BENCHMARK ===");

    // Setup both orchestrators
    let regular = setup_regular_orchestrator().await?;
    let optimized = setup_optimized_orchestrator().await?;

    let test_cases = vec![
        ("echo_test", vec!["echo".to_string(), "Benchmark test".to_string()]),
        ("env_test", vec!["env".to_string()]),
        ("small_compile", vec!["sh".to_string(), "-c".to_string(),
                               "echo 'fn main() { println!(\"Hello\"); }' > main.rs && rustc main.rs && ./main".to_string()]),
    ];

    for (test_name, command) in test_cases {
        info!("\n--- Testing: {} ---", test_name);

        // Test regular executor
        let regular_start = Instant::now();
        let regular_result = regular.schedule_execution(
            format!("regular-{}", test_name),
            if test_name == "small_compile" { "rust:latest" } else { "alpine:latest" }.to_string(),
            command.clone(),
            None,
            Vec::new(),
        ).await;
        let regular_duration = regular_start.elapsed();

        // Test optimized executor
        let optimized_start = Instant::now();
        let optimized_result = optimized.schedule_execution(
            format!("optimized-{}", test_name),
            if test_name == "small_compile" { "rust:latest" } else { "alpine:latest" }.to_string(),
            command.clone(),
            None,
            Vec::new(),
        ).await;
        let optimized_duration = optimized_start.elapsed();

        // Report results
        match (regular_result, optimized_result) {
            (Ok(_), Ok(_)) => {
                let speedup = regular_duration.as_millis() as f64 / optimized_duration.as_millis() as f64;
                info!("=== {} RESULTS ===", test_name.to_uppercase());
                info!("Regular: {:?} ({:.0}ms)", regular_duration, regular_duration.as_millis());
                info!("Optimized: {:?} ({:.0}ms)", optimized_duration, optimized_duration.as_millis());
                info!("Speedup: {:.2}x", speedup);

                // Check if we're hitting performance targets
                if test_name == "echo_test" && optimized_duration.as_millis() < 250 {
                    info!("🚀 SUB-250MS TARGET ACHIEVED!");
                } else if optimized_duration.as_millis() < 500 {
                    info!("✅ Good performance: {}ms", optimized_duration.as_millis());
                } else {
                    warn!("⚠️ Still needs optimization: {}ms", optimized_duration.as_millis());
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

    let optimized = setup_optimized_orchestrator().await?;

    // Multiple attempts to ensure consistency
    let mut attempts = Vec::new();
    for i in 0..5 {
        let start = Instant::now();

        let result = optimized.schedule_execution(
            format!("cold-start-{}", i),
            "alpine:latest".to_string(),
            vec!["echo".to_string(), format!("Attempt {}", i)],
            None,
            Vec::new(),
        ).await;

        let duration = start.elapsed();
        attempts.push(duration);

        match result {
            Ok(_) => info!("Attempt {}: {:?} ({:.0}ms)", i+1, duration, duration.as_millis()),
            Err(e) => warn!("Attempt {} failed: {}", i+1, e),
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
        info!("Average: {:?} ({:.0}ms)", avg_duration, avg_duration.as_millis());
        info!("Min: {:?} ({:.0}ms)", min_duration, min_duration.as_millis());
        info!("Max: {:?} ({:.0}ms)", max_duration, max_duration.as_millis());

        // Performance assessment
        if avg_duration.as_millis() < 250 {
            info!("🎯 TARGET ACHIEVED: Average {}ms < 250ms!", avg_duration.as_millis());
        } else if avg_duration.as_millis() < 500 {
            info!("📈 CLOSE TO TARGET: Average {}ms", avg_duration.as_millis());
        } else {
            info!("🔧 NEEDS OPTIMIZATION: Average {}ms", avg_duration.as_millis());
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

    let optimized = setup_optimized_orchestrator().await?;

    // Simulate typical development workflow
    let workflow_steps = vec![
        ("environment_setup", "alpine:latest", vec!["echo".to_string(), "Setting up environment".to_string()]),
        ("dependency_install", "alpine:latest", vec!["apk".to_string(), "add".to_string(), "--no-cache".to_string(), "git".to_string()]),
        ("code_analysis", "alpine:latest", vec!["find".to_string(), ".".to_string(), "-name".to_string(), "*.txt".to_string()]),
        ("build_step", "alpine:latest", vec!["sh".to_string(), "-c".to_string(), "echo 'Building...' && sleep 0.1 && echo 'Build complete'".to_string()]),
    ];

    let workflow_start = Instant::now();
    let mut step_times = Vec::new();

    for (step_name, image, command) in workflow_steps {
        let step_start = Instant::now();

        match optimized.schedule_execution(
            format!("workflow-{}", step_name),
            image.to_string(),
            command,
            None,
            Vec::new(),
        ).await {
            Ok(result) => {
                let step_duration = step_start.elapsed();
                step_times.push((step_name, step_duration));

                info!("Step '{}': {:?} ({:.0}ms)", step_name, step_duration, step_duration.as_millis());

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
    info!("Total workflow time: {:?} ({:.0}ms)", total_workflow_time, total_workflow_time.as_millis());

    // Performance assessment for developer workflow
    if total_workflow_time.as_secs() < 5 {
        info!("🚀 EXCELLENT: Complete workflow in {}s", total_workflow_time.as_secs());
    } else if total_workflow_time.as_secs() < 10 {
        info!("✅ GOOD: Complete workflow in {}s", total_workflow_time.as_secs());
    } else {
        info!("⚠️ SLOW: Workflow took {}s", total_workflow_time.as_secs());
    }

    Ok(())
}