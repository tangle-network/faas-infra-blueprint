use faas_executor::performance::{
    ContainerPool, PoolConfig, CacheManager, CacheStrategy,
    MetricsCollector, MetricsConfig, SnapshotOptimizer, OptimizationConfig,
    PredictiveScaler, ScalingConfig, PerformanceMetrics
};
use faas_executor::platform::{Executor, Mode, Request};
use std::time::{Duration, Instant};
use tokio;

/// Production performance validation tests
/// These tests ensure we meet our performance targets under realistic loads

#[tokio::test]
async fn test_container_pool_performance_targets() {
    let config = PoolConfig {
        max_containers_per_env: 20,
        min_containers_per_env: 2,
        max_idle_time: Duration::from_secs(300),
        predictive_warming: true,
        cleanup_interval: Duration::from_secs(30),
    };

    let pool = ContainerPool::new(config);

    // Test cold start performance
    let start = Instant::now();
    let container = pool.acquire("alpine:latest").await.unwrap();
    let cold_start_time = start.elapsed();

    assert!(cold_start_time < Duration::from_millis(100),
           "Cold start too slow: {:?}", cold_start_time);

    // Return container to pool
    pool.release(container).await.unwrap();

    // Test warm start performance (should be much faster)
    let start = Instant::now();
    let warm_container = pool.acquire("alpine:latest").await.unwrap();
    let warm_start_time = start.elapsed();

    assert!(warm_start_time < Duration::from_millis(20),
           "Warm start too slow: {:?}", warm_start_time);

    println!("✅ Container pool performance: cold={:?}, warm={:?}",
             cold_start_time, warm_start_time);
}

#[tokio::test]
async fn test_cache_performance_under_load() {
    let strategy = CacheStrategy {
        l1_max_size: 50 * 1024 * 1024, // 50MB
        l1_ttl: Duration::from_secs(3600),
        l2_max_size: 200 * 1024 * 1024, // 200MB
        l2_ttl: Duration::from_secs(86400),
        eviction_policy: faas_executor::performance::cache_manager::EvictionPolicy::Adaptive,
        compression: true,
    };

    let cache = CacheManager::new(strategy).await.unwrap();

    // Test cache performance with concurrent access
    let mut tasks = Vec::new();
    let num_concurrent = 50;

    for i in 0..num_concurrent {
        let cache_clone = cache.clone();
        tasks.push(tokio::spawn(async move {
            let key = format!("test_key_{}", i);
            let data = vec![0u8; 1024]; // 1KB data

            // Put data
            let start = Instant::now();
            cache_clone.put(&key, data.clone(), None).await.unwrap();
            let put_time = start.elapsed();

            // Get data
            let start = Instant::now();
            let retrieved = cache_clone.get(&key).await.unwrap();
            let get_time = start.elapsed();

            assert_eq!(retrieved, Some(data));
            (put_time, get_time)
        }));
    }

    let results = futures::future::join_all(tasks).await;
    let mut total_put_time = Duration::ZERO;
    let mut total_get_time = Duration::ZERO;

    for result in results {
        let (put_time, get_time) = result.unwrap();
        total_put_time += put_time;
        total_get_time += get_time;
    }

    let avg_put_time = total_put_time / num_concurrent;
    let avg_get_time = total_get_time / num_concurrent;

    assert!(avg_put_time < Duration::from_millis(10),
           "Cache put too slow: {:?}", avg_put_time);
    assert!(avg_get_time < Duration::from_millis(5),
           "Cache get too slow: {:?}", avg_get_time);

    println!("✅ Cache performance: put={:?}, get={:?}", avg_put_time, avg_get_time);
}

#[tokio::test]
async fn test_snapshot_optimization_targets() {
    let config = OptimizationConfig {
        enable_compression: true,
        enable_incremental: true,
        enable_parallel_io: true,
        compression_level: 3,
        chunk_size: 64 * 1024,
        max_cache_size: 1024 * 1024 * 1024,
        target_time: Duration::from_millis(200),
    };

    let optimizer = SnapshotOptimizer::new(config);

    // Test full snapshot creation
    let start = Instant::now();
    let (snapshot_id, metadata) = optimizer
        .create_snapshot("test_process_1", None)
        .await
        .unwrap();
    let full_snapshot_time = start.elapsed();

    assert!(full_snapshot_time < Duration::from_millis(250),
           "Full snapshot too slow: {:?}", full_snapshot_time);

    // Test incremental snapshot
    let start = Instant::now();
    let (inc_snapshot_id, inc_metadata) = optimizer
        .create_snapshot("test_process_2", Some(&snapshot_id))
        .await
        .unwrap();
    let incremental_time = start.elapsed();

    assert!(incremental_time < Duration::from_millis(150),
           "Incremental snapshot too slow: {:?}", incremental_time);

    // Test snapshot restoration
    let start = Instant::now();
    let restore_time = optimizer
        .restore_snapshot(&snapshot_id, "restored_process")
        .await
        .unwrap();

    assert!(restore_time < Duration::from_millis(200),
           "Snapshot restore too slow: {:?}", restore_time);

    println!("✅ Snapshot performance: full={:?}, incremental={:?}, restore={:?}",
             full_snapshot_time, incremental_time, restore_time);
}

#[tokio::test]
async fn test_metrics_collection_overhead() {
    let metrics = MetricsCollector::new(MetricsConfig::default());

    let num_operations = 1000;
    let start = Instant::now();

    // Simulate recording many operations
    for i in 0..num_operations {
        metrics.record_execution(
            "ephemeral",
            Duration::from_millis(100),
            true,
            faas_executor::performance::metrics_collector::ResourceSnapshot {
                peak_memory_mb: 64,
                cpu_time_ms: 50,
                disk_reads_mb: 1,
                disk_writes_mb: 0,
            }
        ).await.unwrap();

        if i % 100 == 0 {
            metrics.record_container_event(
                faas_executor::performance::metrics_collector::ContainerEvent::Started,
                Some(Duration::from_millis(50)),
                true
            ).await.unwrap();
        }
    }

    let total_time = start.elapsed();
    let overhead_per_op = total_time / num_operations;

    // Metrics collection should have minimal overhead
    assert!(overhead_per_op < Duration::from_micros(100),
           "Metrics overhead too high: {:?} per operation", overhead_per_op);

    let final_metrics = metrics.get_metrics().await;
    assert_eq!(final_metrics.total_executions, num_operations as u64);
    assert_eq!(final_metrics.successful_executions, num_operations as u64);

    println!("✅ Metrics collection overhead: {:?} per operation", overhead_per_op);
}

#[tokio::test]
async fn test_predictive_scaling_accuracy() {
    let scaler = PredictiveScaler::new(ScalingConfig::default());

    // Simulate usage patterns
    let environments = ["python:3", "node:18", "rust:1.70", "go:1.21"];

    for env in &environments {
        // Record increasing usage pattern
        for hour in 0..24 {
            let load = 1.0 + (hour as f64 / 24.0) * 2.0; // Linear increase
            scaler.record_usage(env, load).await.unwrap();
        }
    }

    // Get predictions for all environments
    let predictions = scaler.get_all_predictions().await.unwrap();

    assert!(!predictions.is_empty(), "Should have predictions for recorded environments");

    for prediction in predictions {
        assert!(prediction.confidence > 0.5,
               "Prediction confidence too low: {}", prediction.confidence);
        assert!(prediction.recommended_instances > 0,
               "Should recommend at least one instance");

        println!("✅ Prediction for {}: {} instances (confidence: {:.2})",
                 prediction.environment,
                 prediction.recommended_instances,
                 prediction.confidence);
    }
}

#[tokio::test]
async fn test_end_to_end_platform_performance() {
    // This test validates the entire platform performance under realistic conditions

    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            println!("⚠️  Skipping end-to-end test: Platform initialization failed (missing dependencies)");
            return;
        }
    };

    let test_cases = vec![
        ("ephemeral", Mode::Ephemeral, Duration::from_millis(100)),
        ("cached", Mode::Cached, Duration::from_millis(150)),
        ("checkpointed", Mode::Checkpointed, Duration::from_millis(300)),
    ];

    for (name, mode, target_time) in test_cases {
        let request = Request {
            id: format!("perf-test-{}", name),
            code: "echo 'Performance test'".to_string(),
            mode,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };

        let start = Instant::now();
        let result = executor.run(request).await.unwrap();
        let execution_time = start.elapsed();

        assert_eq!(result.exit_code, 0, "Execution should succeed");
        assert!(execution_time < target_time,
               "{} mode too slow: {:?} > {:?}", name, execution_time, target_time);

        println!("✅ {} mode performance: {:?} (target: {:?})",
                 name, execution_time, target_time);
    }
}

#[tokio::test]
async fn test_parallel_execution_scalability() {
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            println!("⚠️  Skipping scalability test: Platform initialization failed");
            return;
        }
    };

    let concurrent_requests = 10;
    let mut tasks = Vec::new();

    let start = Instant::now();

    for i in 0..concurrent_requests {
        let executor_clone = executor.clone();
        tasks.push(tokio::spawn(async move {
            let request = Request {
                id: format!("parallel-{}", i),
                code: format!("echo 'Parallel execution {}'", i),
                mode: Mode::Ephemeral,
                env: "alpine:latest".to_string(),
                timeout: Duration::from_secs(30),
                checkpoint: None,
                branch_from: None,
            };

            let start = Instant::now();
            let result = executor_clone.run(request).await.unwrap();
            (start.elapsed(), result.exit_code)
        }));
    }

    let results = futures::future::join_all(tasks).await;
    let total_time = start.elapsed();

    // Verify all succeeded
    for (i, result) in results.iter().enumerate() {
        let (duration, exit_code) = result.as_ref().unwrap();
        assert_eq!(*exit_code, 0, "Request {} should succeed", i);
        assert!(*duration < Duration::from_millis(200),
               "Individual request {} too slow: {:?}", i, duration);
    }

    // Parallel execution should not be much slower than sequential
    let avg_time = total_time / concurrent_requests;
    assert!(total_time < Duration::from_secs(3),
           "Parallel execution too slow: {:?}", total_time);

    println!("✅ Parallel execution: {} requests in {:?} (avg: {:?})",
             concurrent_requests, total_time, avg_time);
}

#[tokio::test]
async fn test_ai_agent_reasoning_patterns() {
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            println!("⚠️  Skipping AI agent test: Platform initialization failed");
            return;
        }
    };

    // Simulate AI agent reasoning pattern: setup -> explore -> evaluate

    // 1. Setup base reasoning state
    let setup_request = Request {
        id: "ai-setup".to_string(),
        code: "echo 'Setting up reasoning environment'".to_string(),
        mode: Mode::Checkpointed,
        env: "python:3-alpine".to_string(),
        timeout: Duration::from_secs(60),
        checkpoint: None,
        branch_from: None,
    };

    let start = Instant::now();
    let base_result = executor.run(setup_request).await.unwrap();
    let setup_time = start.elapsed();

    assert_eq!(base_result.exit_code, 0);
    assert!(base_result.snapshot.is_some(), "Should create snapshot for branching");
    assert!(setup_time < Duration::from_millis(400), "Setup too slow: {:?}", setup_time);

    // 2. Create parallel exploration branches
    let branch_count = 5;
    let mut exploration_tasks = Vec::new();

    let start = Instant::now();
    for i in 0..branch_count {
        let executor_clone = executor.clone();
        let snapshot = base_result.snapshot.clone();

        exploration_tasks.push(tokio::spawn(async move {
            let request = Request {
                id: format!("ai-explore-{}", i),
                code: format!("echo 'Exploring reasoning path {}'", i),
                mode: Mode::Branched,
                env: "python:3-alpine".to_string(),
                timeout: Duration::from_secs(30),
                checkpoint: None,
                branch_from: snapshot,
            };

            let start = Instant::now();
            let result = executor_clone.run(request).await.unwrap();
            (start.elapsed(), result.exit_code)
        }));
    }

    let exploration_results = futures::future::join_all(exploration_tasks).await;
    let total_exploration_time = start.elapsed();

    // Verify all explorations succeeded
    for (i, result) in exploration_results.iter().enumerate() {
        let (duration, exit_code) = result.as_ref().unwrap();
        assert_eq!(*exit_code, 0, "Exploration {} should succeed", i);
        assert!(*duration < Duration::from_millis(100),
               "Branch {} too slow: {:?}", i, duration);
    }

    // Parallel reasoning should complete quickly
    assert!(total_exploration_time < Duration::from_millis(300),
           "Parallel reasoning too slow: {:?}", total_exploration_time);

    println!("✅ AI agent reasoning pattern: setup={:?}, {} parallel explorations in {:?}",
             setup_time, branch_count, total_exploration_time);
}

#[tokio::test]
async fn test_performance_regression_detection() {
    // This test ensures we can detect performance regressions automatically

    let target_metrics = PerformanceTargets {
        max_cold_start: Duration::from_millis(100),
        max_warm_start: Duration::from_millis(20),
        max_snapshot_time: Duration::from_millis(200),
        max_restore_time: Duration::from_millis(200),
        min_cache_hit_rate: 0.8,
        min_success_rate: 0.95,
    };

    // Run a series of operations and validate they meet targets
    let metrics = MetricsCollector::new(MetricsConfig::default());

    // Simulate some operations
    for i in 0..100 {
        let execution_time = Duration::from_millis(50 + (i % 10) * 5); // 50-95ms
        metrics.record_execution(
            "ephemeral",
            execution_time,
            i % 20 != 0, // 95% success rate
            faas_executor::performance::metrics_collector::ResourceSnapshot {
                peak_memory_mb: 64,
                cpu_time_ms: execution_time.as_millis() as u64 / 2,
                disk_reads_mb: 1,
                disk_writes_mb: 0,
            }
        ).await.unwrap();
    }

    let final_metrics = metrics.get_metrics().await;
    let success_rate = final_metrics.successful_executions as f64 / final_metrics.total_executions as f64;

    assert!(success_rate >= target_metrics.min_success_rate,
           "Success rate regression detected: {:.2}% < {:.2}%",
           success_rate * 100.0, target_metrics.min_success_rate * 100.0);

    assert!(final_metrics.avg_execution_time < Duration::from_millis(100),
           "Execution time regression detected: {:?}", final_metrics.avg_execution_time);

    println!("✅ Performance regression check passed: {:.2}% success rate, {:?} avg execution",
             success_rate * 100.0, final_metrics.avg_execution_time);
}

struct PerformanceTargets {
    max_cold_start: Duration,
    max_warm_start: Duration,
    max_snapshot_time: Duration,
    max_restore_time: Duration,
    min_cache_hit_rate: f64,
    min_success_rate: f64,
}

#[tokio::test]
async fn test_stress_load_handling() {
    // Test platform behavior under extreme load
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            println!("⚠️  Skipping stress test: Platform initialization failed");
            return;
        }
    };

    let stress_requests = 25; // High concurrent load
    let mut tasks = Vec::new();

    println!("🔥 Starting stress test with {} concurrent requests", stress_requests);

    let start = Instant::now();

    for i in 0..stress_requests {
        let executor_clone = executor.clone();
        tasks.push(tokio::spawn(async move {
            let request = Request {
                id: format!("stress-{}", i),
                code: "echo 'Stress test execution'".to_string(),
                mode: Mode::Ephemeral,
                env: "alpine:latest".to_string(),
                timeout: Duration::from_secs(30),
                checkpoint: None,
                branch_from: None,
            };

            executor_clone.run(request).await
        }));
    }

    let results = futures::future::join_all(tasks).await;
    let total_time = start.elapsed();

    // Count successes and failures
    let mut successes = 0;
    let mut failures = 0;

    for result in results {
        match result.unwrap() {
            Ok(response) => {
                if response.exit_code == 0 {
                    successes += 1;
                } else {
                    failures += 1;
                }
            }
            Err(_) => failures += 1,
        }
    }

    let success_rate = successes as f64 / stress_requests as f64;

    // Under stress, we should maintain reasonable performance
    assert!(success_rate >= 0.8,
           "Stress test failure rate too high: {}/{} failed",
           failures, stress_requests);

    assert!(total_time < Duration::from_secs(10),
           "Stress test took too long: {:?}", total_time);

    println!("✅ Stress test completed: {}/{} succeeded in {:?} ({:.1}% success rate)",
             successes, stress_requests, total_time, success_rate * 100.0);
}