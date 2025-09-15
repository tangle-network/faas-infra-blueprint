use faas_common::SandboxExecutor;
use faas_executor::docktopus::DockerBuilder;
use faas_executor::{DockerExecutor, Executor, WarmContainer};
use faas_executor::executor::{ExecutionStrategy, ContainerStrategy};
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
        Executor::new(strategy).await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to create executor: {}", e))?
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
        ("echo", vec!["echo".to_string(), "Hello Fast FaaS!".to_string()]),
        ("date", vec!["date".to_string()]),
        ("uname", vec!["uname".to_string(), "-a".to_string()]),
    ];

    for (test_name, command) in test_cases {
        info!("\n--- Testing: {} ---", test_name);

        // Test regular executor (3 runs for average)
        let mut regular_times = Vec::new();
        for i in 0..3 {
            let start = Instant::now();

            match regular_orchestrator.schedule_execution(
                format!("regular-{}-{}", test_name, i),
                "alpine:latest".to_string(),
                command.clone(),
                None,
                Vec::new(),
            ).await {
                Ok(result) => {
                    let duration = start.elapsed();
                    regular_times.push(duration);
                    info!("Regular run {}: {:?}", i+1, duration);

                    if let Some(response) = result.response {
                        info!("Regular output: {}", String::from_utf8_lossy(&response).trim());
                    }
                }
                Err(e) => warn!("Regular execution failed: {}", e),
            }
        }

        // Test fast executor (3 runs for average)
        let mut fast_times = Vec::new();
        for i in 0..3 {
            let start = Instant::now();

            match fast_orchestrator.schedule_execution(
                format!("fast-{}-{}", test_name, i),
                "alpine:latest".to_string(),
                command.clone(),
                None,
                Vec::new(),
            ).await {
                Ok(result) => {
                    let duration = start.elapsed();
                    fast_times.push(duration);
                    info!("Fast run {}: {:?}", i+1, duration);

                    if let Some(response) = result.response {
                        info!("Fast output: {}", String::from_utf8_lossy(&response).trim());
                    }
                }
                Err(e) => warn!("Fast execution failed: {}", e),
            }
        }

        // Calculate averages
        if !regular_times.is_empty() && !fast_times.is_empty() {
            let regular_avg = regular_times.iter().sum::<std::time::Duration>() / regular_times.len() as u32;
            let fast_avg = fast_times.iter().sum::<std::time::Duration>() / fast_times.len() as u32;

            let speedup = regular_avg.as_millis() as f64 / fast_avg.as_millis() as f64;

            info!("=== {} RESULTS ===", test_name.to_uppercase());
            info!("Regular average: {:?} ({:.0}ms)", regular_avg, regular_avg.as_millis());
            info!("Fast average: {:?} ({:.0}ms)", fast_avg, fast_avg.as_millis());
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
            let result = orch.schedule_execution(
                format!("burst-{}", i),
                "alpine:latest".to_string(),
                vec!["echo".to_string(), format!("Burst task {}", i)],
                None,
                Vec::new(),
            ).await;
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
    info!("Total time for {} concurrent executions: {:?}", num_concurrent, total_duration);

    let successful = results.iter().filter(|(_, result, _)| result.is_ok()).count();
    let failed = results.len() - successful;

    info!("Successful: {}, Failed: {}", successful, failed);

    if successful > 0 {
        let task_times: Vec<_> = results.iter()
            .filter(|(_, result, _)| result.is_ok())
            .map(|(_, _, duration)| *duration)
            .collect();

        let avg_task_time = task_times.iter().sum::<std::time::Duration>() / task_times.len() as u32;
        let min_task_time = task_times.iter().min().unwrap();
        let max_task_time = task_times.iter().max().unwrap();

        info!("Average task time: {:?} ({:.0}ms)", avg_task_time, avg_task_time.as_millis());
        info!("Min task time: {:?} ({:.0}ms)", min_task_time, min_task_time.as_millis());
        info!("Max task time: {:?} ({:.0}ms)", max_task_time, max_task_time.as_millis());

        // Test if we achieved ultra-fast execution (target <100ms average)
        if avg_task_time.as_millis() < 100 {
            info!("🚀 ULTRA-FAST ACHIEVED: Average {}ms < 100ms target!", avg_task_time.as_millis());
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
    let cold_result = fast_orchestrator.schedule_execution(
        "cold-start".to_string(),
        "alpine:latest".to_string(),
        vec!["echo".to_string(), "Cold start".to_string()],
        None,
        Vec::new(),
    ).await;
    let cold_duration = cold_start.elapsed();

    // Give time for pool to warm up
    info!("Warming up container pool...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Warm start test (after pool is ready)
    info!("Testing warm start...");
    let warm_start = Instant::now();
    let warm_result = fast_orchestrator.schedule_execution(
        "warm-start".to_string(),
        "alpine:latest".to_string(),
        vec!["echo".to_string(), "Warm start".to_string()],
        None,
        Vec::new(),
    ).await;
    let warm_duration = warm_start.elapsed();

    info!("=== START COMPARISON RESULTS ===");

    match &cold_result {
        Ok(_) => info!("Cold start: {:?} ({:.0}ms)", cold_duration, cold_duration.as_millis()),
        Err(e) => warn!("Cold start failed: {}", e),
    }

    match &warm_result {
        Ok(_) => info!("Warm start: {:?} ({:.0}ms)", warm_duration, warm_duration.as_millis()),
        Err(e) => warn!("Warm start failed: {}", e),
    }

    if cold_result.is_ok() && warm_result.is_ok() {
        let speedup = cold_duration.as_millis() as f64 / warm_duration.as_millis() as f64;
        info!("Warm start speedup: {:.2}x", speedup);

        if warm_duration.as_millis() < 50 {
            info!("🚀 LIGHTNING FAST: {}ms warm start!", warm_duration.as_millis());
        } else if warm_duration.as_millis() < 100 {
            info!("⚡ ULTRA-FAST: {}ms warm start!", warm_duration.as_millis());
        }
    }

    Ok(())
}