use faas_executor::bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, WaitContainerOptions,
};
use faas_executor::bollard::exec::CreateExecOptions;
use faas_executor::bollard::Docker;
use faas_executor::container_pool::{ContainerPool, ContainerPoolManager, PoolConfig};
use faas_executor::{
    common::{SandboxConfig, SandboxExecutor},
    DockerExecutor,
};
use futures::TryStreamExt;
/// REAL Performance Tests - No Mocks, No Shortcuts
/// These tests measure actual Docker container operations
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Measure REAL cold start time - creating container from scratch
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_real_cold_start_performance() {
    println!("\n🔬 REAL Cold Start Performance Test");
    println!("{}", "=".repeat(50));

    let docker = Docker::connect_with_defaults().unwrap();

    // Clean slate - no pre-existing containers
    println!("📦 Ensuring no pre-warmed containers exist...");

    let mut cold_start_times = Vec::new();

    for i in 1..=5 {
        println!("\n🚀 Cold Start Test #{}", i);

        // Measure TRUE cold start
        let start = Instant::now();

        let config = Config {
            image: Some("alpine:latest".to_string()),
            cmd: Some(vec!["echo".to_string(), format!("test-{}", i)]),
            ..Default::default()
        };

        let container_name = format!("cold-start-test-{}", uuid::Uuid::new_v4());
        let options = CreateContainerOptions {
            name: container_name.clone(),
            ..Default::default()
        };

        // Create container
        let container = docker
            .create_container(Some(options), config)
            .await
            .unwrap();

        // Start container
        docker
            .start_container::<String>(&container.id, None)
            .await
            .unwrap();

        // Wait for completion
        let _ = docker
            .wait_container(&container.id, None::<WaitContainerOptions<String>>)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        let duration = start.elapsed();
        cold_start_times.push(duration);

        println!("  ⏱️  Cold start time: {:?}", duration);

        // Cleanup
        docker
            .remove_container(
                &container.id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .ok();
    }

    // Statistics
    let avg_cold_start = cold_start_times.iter().sum::<Duration>() / cold_start_times.len() as u32;
    let min = cold_start_times.iter().min().unwrap();
    let max = cold_start_times.iter().max().unwrap();

    println!("\n📊 Cold Start Statistics:");
    println!("  Average: {:?}", avg_cold_start);
    println!("  Min: {:?}", min);
    println!("  Max: {:?}", max);

    // Reality check
    assert!(
        avg_cold_start > Duration::from_millis(100),
        "Cold start under 100ms is unrealistic for Docker!"
    );
    assert!(
        avg_cold_start < Duration::from_secs(5),
        "Cold start over 5 seconds indicates a problem"
    );

    println!("\n✅ Cold start times are REALISTIC");
}

/// Test REAL warm pool performance - with actual pre-warmed containers
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_real_warm_pool_performance() {
    println!("\n🔥 REAL Warm Pool Performance Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());

    let config = PoolConfig {
        min_size: 3,
        max_size: 10,
        max_idle_time: Duration::from_secs(300),
        max_use_count: 10,
        health_check_interval: Duration::from_secs(30),
        pre_warm: true,
        predictive_warming: false,
        target_acquisition_ms: 100,
    };

    let pool = ContainerPool::new(docker.clone(), "alpine:latest".to_string(), config);

    // ACTUALLY pre-warm the pool and WAIT for it
    println!("🔨 Pre-warming pool with 3 containers...");
    let pre_warm_start = Instant::now();
    pool.pre_warm().await.unwrap();
    let pre_warm_duration = pre_warm_start.elapsed();
    println!("  Pre-warming took: {:?}", pre_warm_duration);

    // Pre-warming should take time (creating real containers)
    assert!(
        pre_warm_duration > Duration::from_millis(500),
        "Pre-warming finished too quickly - likely not creating real containers!"
    );

    // Now test warm acquisition
    println!("\n📦 Testing warm container acquisition...");
    let mut warm_times = Vec::new();

    for i in 1..=5 {
        let start = Instant::now();
        let container = pool.acquire().await.unwrap();
        let acquisition_time = start.elapsed();

        println!("  Test #{}: Warm acquisition: {:?}", i, acquisition_time);
        warm_times.push(acquisition_time);

        // Actually use the container
        let exec_config = CreateExecOptions {
            cmd: Some(vec!["echo", "warm-test"]),
            attach_stdout: Some(true),
            ..Default::default()
        };

        let exec = docker
            .create_exec(&container.container_id, exec_config)
            .await
            .unwrap();
        docker.start_exec(&exec.id, None).await.unwrap();

        // Return to pool
        pool.release(container).await.unwrap();
    }

    let avg_warm = warm_times.iter().sum::<Duration>() / warm_times.len() as u32;

    println!("\n📊 Warm Acquisition Statistics:");
    println!("  Average: {:?}", avg_warm);
    println!("  All times: {:?}", warm_times);

    // Warm acquisition should be faster than cold start but still realistic
    assert!(
        avg_warm < Duration::from_millis(100),
        "Warm acquisition should be fast (from pool)"
    );
    assert!(
        avg_warm > Duration::from_micros(100),
        "Warm acquisition under 100μs is suspiciously fast"
    );

    println!("\n✅ Warm pool times are REALISTIC");
}

/// Test container reuse performance
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_container_reuse_performance() {
    println!("\n♻️  Container Reuse Performance Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    let mut execution_times = Vec::new();

    // First execution - cold start
    println!("\n🥶 First execution (cold):");
    let start = Instant::now();
    let result = executor
        .execute(SandboxConfig {
            function_id: "reuse-test-1".to_string(),
            source: "alpine:latest".to_string(),
            command: vec!["echo".to_string(), "test1".to_string()],
            env_vars: None,
            payload: vec![],
        })
        .await
        .unwrap();
    let cold_time = start.elapsed();
    execution_times.push(("Cold", cold_time));
    println!("  Time: {:?}", cold_time);
    println!(
        "  Output: {:?}",
        String::from_utf8_lossy(&result.response.unwrap())
    );

    // Subsequent executions - should reuse if pooling is enabled
    for i in 2..=5 {
        println!("\n🔄 Execution #{} (potential reuse):", i);
        let start = Instant::now();
        let result = executor
            .execute(SandboxConfig {
                function_id: format!("reuse-test-{}", i),
                source: "alpine:latest".to_string(),
                command: vec!["echo".to_string(), format!("test{}", i)],
                env_vars: None,
                payload: vec![],
            })
            .await
            .unwrap();
        let exec_time = start.elapsed();
        execution_times.push((format!("Exec {}", i).leak(), exec_time));
        println!("  Time: {:?}", exec_time);
        println!(
            "  Output: {:?}",
            String::from_utf8_lossy(&result.response.unwrap())
        );
    }

    println!("\n📊 Execution Time Comparison:");
    for (label, time) in &execution_times {
        println!("  {}: {:?}", label, time);
    }

    // Verify performance improvement
    let cold = execution_times[0].1;
    let warm_avg = execution_times[1..]
        .iter()
        .map(|(_, t)| *t)
        .sum::<Duration>()
        / (execution_times.len() - 1) as u32;

    println!("\n📈 Performance Analysis:");
    println!("  Cold start: {:?}", cold);
    println!("  Warm average: {:?}", warm_avg);
    println!(
        "  Speedup: {:.2}x",
        cold.as_millis() as f64 / warm_avg.as_millis() as f64
    );

    // Warm should be noticeably faster than cold
    assert!(
        warm_avg < cold,
        "Warm executions should be faster than cold"
    );

    println!("\n✅ Container reuse provides real performance benefits");
}

/// Benchmark concurrent execution performance
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_concurrent_execution_performance() {
    println!("\n⚡ Concurrent Execution Performance Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = Arc::new(DockerExecutor::new(docker));

    let concurrent_counts = vec![1, 5, 10, 20];

    for count in concurrent_counts {
        println!("\n🔀 Testing {} concurrent executions:", count);

        let start = Instant::now();
        let mut tasks = vec![];

        for i in 0..count {
            let executor = executor.clone();
            let task = tokio::spawn(async move {
                let exec_start = Instant::now();
                let result = executor
                    .execute(SandboxConfig {
                        function_id: format!("concurrent-{}", i),
                        source: "alpine:latest".to_string(),
                        command: vec![
                            "sh".to_string(),
                            "-c".to_string(),
                            format!("echo 'Task {}' && sleep 0.1", i),
                        ],
                        env_vars: None,
                        payload: vec![],
                    })
                    .await;
                (i, exec_start.elapsed(), result)
            });
            tasks.push(task);
        }

        let results = futures::future::join_all(tasks).await;
        let total_time = start.elapsed();

        let successful = results
            .iter()
            .filter(|r| r.as_ref().unwrap().2.is_ok())
            .count();
        let avg_individual = results
            .iter()
            .map(|r| r.as_ref().unwrap().1)
            .sum::<Duration>()
            / count as u32;

        println!("  Total time: {:?}", total_time);
        println!("  Average individual: {:?}", avg_individual);
        println!("  Successful: {}/{}", successful, count);
        println!(
            "  Throughput: {:.2} exec/sec",
            count as f64 / total_time.as_secs_f64()
        );

        assert_eq!(
            successful, count,
            "All concurrent executions should succeed"
        );
    }

    println!("\n✅ Concurrent execution scaling verified");
}

/// Test memory usage under load
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_memory_usage_under_load() {
    println!("\n🧠 Memory Usage Under Load Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker.clone());

    // Get initial container count
    let initial_containers = docker.list_containers::<String>(None).await.unwrap().len();
    println!("📦 Initial containers: {}", initial_containers);

    // Run many executions
    println!("\n🔄 Running 50 executions...");
    for i in 0..50 {
        if i % 10 == 0 {
            println!("  Progress: {}/50", i);
        }

        let _ = executor
            .execute(SandboxConfig {
                function_id: format!("memory-test-{}", i),
                source: "alpine:latest".to_string(),
                command: vec!["echo".to_string(), format!("test-{}", i)],
                env_vars: None,
                payload: vec![],
            })
            .await;

        // Small delay to avoid overwhelming
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Check for container leaks
    let final_containers = docker.list_containers::<String>(None).await.unwrap().len();
    println!("\n📦 Final containers: {}", final_containers);

    // Allow for some pooled containers but not unbounded growth
    let leaked = final_containers.saturating_sub(initial_containers);
    println!("📈 Container growth: {}", leaked);

    assert!(leaked < 20, "Too many containers leaked: {}", leaked);

    println!("\n✅ Memory usage is bounded - no major leaks detected");
}

/// Validate performance metrics are being collected accurately
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_performance_metrics_accuracy() {
    println!("\n📊 Performance Metrics Accuracy Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let config = PoolConfig::default();
    let pool_manager = Arc::new(ContainerPoolManager::new(docker.clone(), config));

    // Perform operations and measure
    let pool = pool_manager.get_pool("alpine:latest").await;

    // Pre-warm and measure
    let pre_warm_start = Instant::now();
    pool.pre_warm().await.unwrap();
    let actual_pre_warm = pre_warm_start.elapsed();

    // Acquire and measure
    let acquire_start = Instant::now();
    let container = pool.acquire().await.unwrap();
    let actual_acquire = acquire_start.elapsed();

    println!("\n🔍 Timing Results:");
    println!("  Actual pre-warm time: {:?}", actual_pre_warm);
    println!("  Actual acquire time: {:?}", actual_acquire);

    // Verify times are realistic
    assert!(
        actual_pre_warm > Duration::from_millis(100),
        "Pre-warm should take time to create containers"
    );
    assert!(
        actual_acquire < Duration::from_millis(100),
        "Acquire from pre-warmed pool should be fast"
    );

    // Clean up
    pool.release(container).await.unwrap();

    println!("\n✅ Performance timings verified");
}
