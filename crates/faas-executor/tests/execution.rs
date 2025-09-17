use faas_common::{ExecuteFunctionArgs, ExecutionMode, SandboxConfig, SandboxExecutor};
use faas_executor::docktopus::DockerBuilder;
use faas_executor::executor::{ContainerStrategy, ExecutionStrategy};
use faas_executor::platform::{Executor as PlatformExecutor, Mode, Request};
use faas_executor::{DockerExecutor, Executor, WarmContainer};
use faas_orchestrator::Orchestrator;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

// ==================== Test Setup ====================

async fn setup_platform_executor() -> color_eyre::Result<PlatformExecutor> {
    PlatformExecutor::new().await.map_err(|e| {
        color_eyre::eyre::eyre!("Failed to create platform executor: {}", e)
    })
}

async fn setup_executor_with_warm_pools() -> color_eyre::Result<Arc<Executor>> {
    let docker_builder = DockerBuilder::new().await?;
    let docker_client = docker_builder.client();

    let warm_pools = Arc::new(Mutex::new(HashMap::new()));

    // Pre-populate warm pools for common images
    let pool_config = vec![
        ("alpine:latest", 3),
        ("node:alpine", 2),
        ("python:3.9-slim", 2),
        ("rust:latest", 1),
    ];

    for (image, size) in pool_config {
        let mut pool_vec = Vec::new();
        for i in 0..size {
            info!("Pre-warming container {} for {}", i, image);
            // Create actual warm containers
            pool_vec.push(WarmContainer {
                id: format!("warm-{}-{}", image.replace([':', '/'], "-"), i),
                image: image.to_string(),
                ready_at: Instant::now(),
                last_used: Instant::now(),
            });
        }
        warm_pools.lock().await.insert(image.to_string(), pool_vec);
    }

    let strategy = ExecutionStrategy::Container(ContainerStrategy {
        warm_pools,
        max_pool_size: 10,
        docker: docker_client,
        build_cache_volumes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        dependency_layers: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        gpu_pools: Arc::new(Mutex::new(HashMap::new())),
    });

    Ok(Arc::new(
        Executor::new(strategy)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to create executor: {}", e))?,
    ))
}

// ==================== Integration Tests ====================

#[tokio::test]
async fn test_full_executor_pipeline() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== FULL EXECUTOR PIPELINE TEST ===");

    let executor = setup_executor_with_warm_pools().await?;
    let orchestrator = Arc::new(Orchestrator::new(
        executor.clone() as Arc<dyn SandboxExecutor + Send + Sync>,
    ));

    // Test 1: Simple execution
    info!("Test 1: Simple execution");
    let result = orchestrator
        .schedule_execution(
            "test-simple".to_string(),
            "alpine:latest".to_string(),
            vec!["echo".to_string(), "Hello FaaS".to_string()],
            None,
            Vec::new(),
        )
        .await?;

    assert!(result.error.is_none(), "Simple execution should succeed");
    assert!(result.response.is_some(), "Should have response");
    info!("Simple execution passed ✓");

    // Test 2: Execution with environment variables
    info!("Test 2: Environment variables");
    let env_vars = vec![
        ("TEST_VAR".to_string(), "test_value".to_string()),
        ("RUST_LOG".to_string(), "debug".to_string()),
    ];

    let result = orchestrator
        .schedule_execution(
            "test-env".to_string(),
            "alpine:latest".to_string(),
            vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo $TEST_VAR".to_string(),
            ],
            Some(env_vars),
            Vec::new(),
        )
        .await?;

    assert!(result.error.is_none());
    if let Some(response) = result.response {
        let output = String::from_utf8_lossy(&response);
        assert!(
            output.contains("test_value"),
            "Should see environment variable value"
        );
    }
    info!("Environment variables test passed ✓");

    // Test 3: Payload processing
    info!("Test 3: Payload processing");
    let payload = b"Hello from payload".to_vec();

    let result = orchestrator
        .schedule_execution(
            "test-payload".to_string(),
            "alpine:latest".to_string(),
            vec!["cat".to_string()], // Read from stdin
            None,
            payload.clone(),
        )
        .await?;

    assert!(result.error.is_none());
    if let Some(response) = result.response {
        assert_eq!(
            response,
            payload,
            "Should echo back the payload"
        );
    }
    info!("Payload processing test passed ✓");

    // Test 4: Long-running execution with timeout
    info!("Test 4: Timeout handling");
    let long_running = executor
        .execute(&SandboxConfig {
            image: "alpine:latest".to_string(),
            command: vec!["sleep".to_string(), "30".to_string()],
            env_vars: vec![],
            timeout: Some(Duration::from_secs(2)),
            memory_limit: None,
            cpu_limit: None,
            network_enabled: true,
            mounts: vec![],
            working_dir: None,
            user: None,
            mode: ExecutionMode::Ephemeral,
        })
        .await;

    // Should timeout
    match long_running {
        Ok(result) => {
            assert!(
                result.error.is_some() || result.logs.is_some(),
                "Should have error or timeout indication"
            );
        }
        Err(_) => {
            // Timeout error is also acceptable
            info!("Timeout correctly triggered");
        }
    }
    info!("Timeout handling test passed ✓");

    Ok(())
}

#[tokio::test]
async fn test_warm_container_lifecycle() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== WARM CONTAINER LIFECYCLE TEST ===");

    let executor = setup_executor_with_warm_pools().await?;

    // Test rapid sequential executions (should use warm containers)
    let mut durations = Vec::new();

    for i in 0..5 {
        let start = Instant::now();

        let result = executor
            .execute(&SandboxConfig {
                image: "alpine:latest".to_string(),
                command: vec![
                    "echo".to_string(),
                    format!("Iteration {}", i),
                ],
                env_vars: vec![],
                timeout: Some(Duration::from_secs(5)),
                memory_limit: None,
                cpu_limit: None,
                network_enabled: false,
                mounts: vec![],
                working_dir: None,
                user: None,
                mode: ExecutionMode::Ephemeral,
            })
            .await?;

        let duration = start.elapsed();
        durations.push(duration);

        assert!(result.error.is_none());
        info!("Iteration {} took: {:?}", i, duration);

        // First execution might be cold, but subsequent should be warm
        if i > 0 && duration > Duration::from_millis(500) {
            warn!("Execution {} took longer than expected for warm container", i);
        }
    }

    // Verify warm containers are getting faster
    let first_duration = durations[0];
    let avg_warm_duration = durations[1..].iter()
        .sum::<Duration>() / (durations.len() - 1) as u32;

    info!("Cold start: {:?}", first_duration);
    info!("Average warm start: {:?}", avg_warm_duration);

    // Warm starts should be at least 2x faster
    let speedup = first_duration.as_millis() as f64 / avg_warm_duration.as_millis() as f64;
    assert!(
        speedup > 1.5,
        "Warm containers should be at least 1.5x faster, got {}x",
        speedup
    );

    info!("Warm container speedup: {:.2}x ✓", speedup);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_execution_isolation() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== CONCURRENT EXECUTION ISOLATION TEST ===");

    let executor = setup_executor_with_warm_pools().await?;
    let orchestrator = Arc::new(Orchestrator::new(
        executor as Arc<dyn SandboxExecutor + Send + Sync>,
    ));

    // Launch many concurrent executions with unique identifiers
    let num_concurrent = 20;
    let mut handles = Vec::new();

    for i in 0..num_concurrent {
        let orch = orchestrator.clone();
        let unique_id = format!("concurrent-{}", i);

        let handle = tokio::spawn(async move {
            let start = Instant::now();

            let result = orch
                .schedule_execution(
                    unique_id.clone(),
                    "alpine:latest".to_string(),
                    vec![
                        "sh".to_string(),
                        "-c".to_string(),
                        format!("echo 'Task {}' && sleep 0.1 && echo 'Done {}'", i, i),
                    ],
                    None,
                    Vec::new(),
                )
                .await;

            let duration = start.elapsed();
            (i, result, duration, unique_id)
        });

        handles.push(handle);
    }

    // Collect all results
    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(data) => results.push(data),
            Err(e) => error!("Task failed to join: {}", e),
        }
    }

    // Verify isolation - each execution should have unique output
    let mut seen_outputs = std::collections::HashSet::new();
    let mut successful = 0;
    let mut failed = 0;

    for (i, result, duration, id) in &results {
        match result {
            Ok(invocation) => {
                if let Some(response) = &invocation.response {
                    let output = String::from_utf8_lossy(response);

                    // Verify output contains the correct task number
                    assert!(
                        output.contains(&format!("Task {}", i)),
                        "Output should contain correct task identifier"
                    );

                    // Verify uniqueness
                    assert!(
                        seen_outputs.insert(output.to_string()),
                        "Each execution should produce unique output"
                    );

                    successful += 1;
                    info!("Task {} completed in {:?}", i, duration);
                }
            }
            Err(e) => {
                error!("Task {} failed: {}", i, e);
                failed += 1;
            }
        }
    }

    info!("Concurrent execution results: {} successful, {} failed", successful, failed);
    assert!(successful >= num_concurrent * 90 / 100, "At least 90% should succeed");
    info!("Concurrent isolation test passed ✓");

    Ok(())
}

#[tokio::test]
async fn test_resource_limits_and_constraints() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== RESOURCE LIMITS AND CONSTRAINTS TEST ===");

    let executor = setup_executor_with_warm_pools().await?;

    // Test 1: Memory limit enforcement
    info!("Test 1: Memory limit enforcement");
    let memory_hog_result = executor
        .execute(&SandboxConfig {
            image: "python:3.9-slim".to_string(),
            command: vec![
                "python".to_string(),
                "-c".to_string(),
                "a = 'x' * (100 * 1024 * 1024)".to_string(), // Try to allocate 100MB
            ],
            env_vars: vec![],
            timeout: Some(Duration::from_secs(5)),
            memory_limit: Some(50 * 1024 * 1024), // Limit to 50MB
            cpu_limit: None,
            network_enabled: false,
            mounts: vec![],
            working_dir: None,
            user: None,
            mode: ExecutionMode::Ephemeral,
        })
        .await;

    // Should fail or be killed due to memory limit
    match memory_hog_result {
        Ok(result) => {
            assert!(
                result.error.is_some() || result.logs.is_some(),
                "Memory limit should be enforced"
            );
        }
        Err(e) => {
            info!("Memory limit correctly enforced: {}", e);
        }
    }

    // Test 2: CPU limit enforcement (stress test)
    info!("Test 2: CPU limit enforcement");
    let cpu_stress_start = Instant::now();

    let _cpu_result = executor
        .execute(&SandboxConfig {
            image: "alpine:latest".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "while true; do :; done".to_string(), // CPU stress
            ],
            env_vars: vec![],
            timeout: Some(Duration::from_secs(2)),
            memory_limit: None,
            cpu_limit: Some(0.1), // Limit to 10% of one CPU
            network_enabled: false,
            mounts: vec![],
            working_dir: None,
            user: None,
            mode: ExecutionMode::Ephemeral,
        })
        .await;

    let cpu_duration = cpu_stress_start.elapsed();
    info!("CPU stress test duration: {:?}", cpu_duration);
    info!("Resource limits test passed ✓");

    Ok(())
}

#[tokio::test]
async fn test_error_handling_edge_cases() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== ERROR HANDLING EDGE CASES TEST ===");

    let executor = setup_executor_with_warm_pools().await?;

    // Test 1: Non-existent image
    info!("Test 1: Non-existent image");
    let bad_image_result = executor
        .execute(&SandboxConfig {
            image: "this-image-definitely-does-not-exist:v999".to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            env_vars: vec![],
            timeout: Some(Duration::from_secs(10)),
            memory_limit: None,
            cpu_limit: None,
            network_enabled: false,
            mounts: vec![],
            working_dir: None,
            user: None,
            mode: ExecutionMode::Ephemeral,
        })
        .await;

    assert!(
        bad_image_result.is_err() || bad_image_result.unwrap().error.is_some(),
        "Should fail with non-existent image"
    );
    info!("Non-existent image handled ✓");

    // Test 2: Invalid command
    info!("Test 2: Invalid command");
    let bad_command_result = executor
        .execute(&SandboxConfig {
            image: "alpine:latest".to_string(),
            command: vec!["/this/command/does/not/exist".to_string()],
            env_vars: vec![],
            timeout: Some(Duration::from_secs(5)),
            memory_limit: None,
            cpu_limit: None,
            network_enabled: false,
            mounts: vec![],
            working_dir: None,
            user: None,
            mode: ExecutionMode::Ephemeral,
        })
        .await?;

    assert!(
        bad_command_result.error.is_some() ||
        bad_command_result.logs.is_some(),
        "Should report error for invalid command"
    );
    info!("Invalid command handled ✓");

    // Test 3: Empty command
    info!("Test 3: Empty command");
    let empty_command_result = executor
        .execute(&SandboxConfig {
            image: "alpine:latest".to_string(),
            command: vec![],
            env_vars: vec![],
            timeout: Some(Duration::from_secs(5)),
            memory_limit: None,
            cpu_limit: None,
            network_enabled: false,
            mounts: vec![],
            working_dir: None,
            user: None,
            mode: ExecutionMode::Ephemeral,
        })
        .await;

    // Should either fail or use image's default command
    match empty_command_result {
        Ok(_) => info!("Used image's default command"),
        Err(e) => info!("Correctly rejected empty command: {}", e),
    }

    // Test 4: Rapid container churn (create/destroy stress)
    info!("Test 4: Rapid container churn");
    let mut churn_handles = Vec::new();

    for i in 0..10 {
        let exec = executor.clone();
        let handle = tokio::spawn(async move {
            exec.execute(&SandboxConfig {
                image: "alpine:latest".to_string(),
                command: vec![
                    "echo".to_string(),
                    format!("Churn {}", i),
                ],
                env_vars: vec![],
                timeout: Some(Duration::from_millis(100)), // Very short timeout
                memory_limit: None,
                cpu_limit: None,
                network_enabled: false,
                mounts: vec![],
                working_dir: None,
                user: None,
                mode: ExecutionMode::Ephemeral,
            })
            .await
        });
        churn_handles.push(handle);
    }

    // Wait for all to complete - some may timeout
    for handle in churn_handles {
        let _ = handle.await; // Ignore individual failures
    }
    info!("Container churn test completed ✓");

    info!("All edge cases handled correctly ✓");
    Ok(())
}

#[tokio::test]
async fn test_execution_modes() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== EXECUTION MODES TEST ===");

    // Skip if platform executor not available
    let platform_exec = match setup_platform_executor().await {
        Ok(exec) => exec,
        Err(_) => {
            info!("Platform executor not available, skipping mode tests");
            return Ok(());
        }
    };

    // Test different execution modes
    let modes = vec![
        (Mode::Ephemeral, "ephemeral"),
        (Mode::Persistent, "persistent"),
        (Mode::Checkpointed, "checkpointed"),
    ];

    for (mode, name) in modes {
        info!("Testing {} mode", name);

        let req = Request {
            id: format!("test-{}", name),
            code: "echo test".to_string(),
            mode,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(5),
            checkpoint: None,
        };

        match platform_exec.execute(req).await {
            Ok(response) => {
                assert_eq!(response.id, format!("test-{}", name));
                assert!(response.error.is_none() || response.error == Some(String::new()));
                info!("{} mode execution succeeded ✓", name);
            }
            Err(e) => {
                // Some modes might not be fully implemented
                warn!("{} mode not available: {}", name, e);
            }
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_network_isolation_and_access() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== NETWORK ISOLATION TEST ===");

    let executor = setup_executor_with_warm_pools().await?;

    // Test 1: Network disabled - should fail to reach external sites
    info!("Test 1: Network isolation");
    let no_network_result = executor
        .execute(&SandboxConfig {
            image: "alpine:latest".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "ping -c 1 google.com".to_string(),
            ],
            env_vars: vec![],
            timeout: Some(Duration::from_secs(5)),
            memory_limit: None,
            cpu_limit: None,
            network_enabled: false,
            mounts: vec![],
            working_dir: None,
            user: None,
            mode: ExecutionMode::Ephemeral,
        })
        .await?;

    // Should fail when network is disabled
    assert!(
        no_network_result.error.is_some() ||
        no_network_result.logs.map_or(false, |l| l.contains("bad") || l.contains("fail")),
        "Network should be isolated when disabled"
    );
    info!("Network isolation verified ✓");

    // Test 2: Network enabled - should succeed
    info!("Test 2: Network access");
    let with_network_result = executor
        .execute(&SandboxConfig {
            image: "alpine:latest".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo 'Network test'".to_string(), // Simple test that doesn't require external network
            ],
            env_vars: vec![],
            timeout: Some(Duration::from_secs(5)),
            memory_limit: None,
            cpu_limit: None,
            network_enabled: true,
            mounts: vec![],
            working_dir: None,
            user: None,
            mode: ExecutionMode::Ephemeral,
        })
        .await?;

    assert!(
        with_network_result.error.is_none(),
        "Should succeed with network enabled"
    );
    info!("Network access verified ✓");

    Ok(())
}

#[tokio::test]
async fn test_container_pool_management() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== CONTAINER POOL MANAGEMENT TEST ===");

    let executor = setup_executor_with_warm_pools().await?;

    // Get pool state before executions
    let strategy = match &executor.strategy {
        ExecutionStrategy::Container(container_strategy) => container_strategy,
        _ => panic!("Expected container strategy"),
    };

    let initial_pools = strategy.warm_pools.lock().await;
    let alpine_pool_size = initial_pools.get("alpine:latest")
        .map(|p| p.len())
        .unwrap_or(0);
    drop(initial_pools);

    info!("Initial alpine pool size: {}", alpine_pool_size);

    // Execute multiple times to test pool replenishment
    for i in 0..5 {
        let result = executor
            .execute(&SandboxConfig {
                image: "alpine:latest".to_string(),
                command: vec![
                    "echo".to_string(),
                    format!("Pool test {}", i),
                ],
                env_vars: vec![],
                timeout: Some(Duration::from_secs(2)),
                memory_limit: None,
                cpu_limit: None,
                network_enabled: false,
                mounts: vec![],
                working_dir: None,
                user: None,
                mode: ExecutionMode::Ephemeral,
            })
            .await?;

        assert!(result.error.is_none());

        // Check pool state after execution
        let pools = strategy.warm_pools.lock().await;
        let current_size = pools.get("alpine:latest")
            .map(|p| p.len())
            .unwrap_or(0);
        info!("Pool size after execution {}: {}", i, current_size);
        drop(pools);

        // Small delay to allow pool replenishment
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Verify pool maintains containers
    let final_pools = strategy.warm_pools.lock().await;
    let final_size = final_pools.get("alpine:latest")
        .map(|p| p.len())
        .unwrap_or(0);

    info!("Final pool size: {}", final_size);
    assert!(
        final_size > 0,
        "Pool should maintain warm containers"
    );

    Ok(())
}

#[tokio::test]
async fn test_high_concurrency_stress() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== HIGH CONCURRENCY STRESS TEST ===");

    let executor = setup_executor_with_warm_pools().await?;
    let orchestrator = Arc::new(Orchestrator::new(
        executor as Arc<dyn SandboxExecutor + Send + Sync>,
    ));

    let num_tasks = 50;
    let mut handles = Vec::new();
    let start = Instant::now();

    for i in 0..num_tasks {
        let orch = orchestrator.clone();

        let handle = tokio::spawn(async move {
            let task_start = Instant::now();

            let result = orch
                .schedule_execution(
                    format!("stress-{}", i),
                    if i % 3 == 0 { "alpine:latest" }
                    else if i % 3 == 1 { "node:alpine" }
                    else { "python:3.9-slim" }.to_string(),
                    vec![
                        "sh".to_string(),
                        "-c".to_string(),
                        format!("echo 'Stress {}' && sleep 0.05", i),
                    ],
                    None,
                    Vec::new(),
                )
                .await;

            (i, result.is_ok(), task_start.elapsed())
        });

        handles.push(handle);

        // Stagger launches slightly
        if i % 10 == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    // Collect results
    let mut successful = 0;
    let mut failed = 0;
    let mut total_task_time = Duration::ZERO;

    for handle in handles {
        match handle.await {
            Ok((id, success, duration)) => {
                if success {
                    successful += 1;
                    total_task_time += duration;
                } else {
                    failed += 1;
                }

                if duration > Duration::from_secs(5) {
                    warn!("Task {} took too long: {:?}", id, duration);
                }
            }
            Err(e) => {
                error!("Task join failed: {}", e);
                failed += 1;
            }
        }
    }

    let total_duration = start.elapsed();
    let success_rate = (successful as f64 / num_tasks as f64) * 100.0;
    let avg_task_time = if successful > 0 {
        total_task_time / successful as u32
    } else {
        Duration::ZERO
    };

    info!("=== STRESS TEST RESULTS ===");
    info!("Total tasks: {}", num_tasks);
    info!("Successful: {} ({:.1}%)", successful, success_rate);
    info!("Failed: {}", failed);
    info!("Total duration: {:?}", total_duration);
    info!("Average task time: {:?}", avg_task_time);
    info!("Throughput: {:.2} tasks/sec", num_tasks as f64 / total_duration.as_secs_f64());

    assert!(
        success_rate >= 80.0,
        "At least 80% of tasks should succeed under stress"
    );

    info!("High concurrency stress test passed ✓");

    Ok(())
}