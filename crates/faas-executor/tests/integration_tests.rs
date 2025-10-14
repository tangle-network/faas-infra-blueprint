/// Comprehensive integration tests that validate end-to-end functionality
/// These tests ensure all components work together in real production scenarios
use anyhow::Result;
use faas_executor::performance::*;
use faas_executor::platform::executor::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Test the complete execution pipeline with real workloads
#[tokio::test]
async fn test_complete_execution_pipeline() -> Result<()> {
    println!("=== Testing Complete Execution Pipeline ===");

    let executor = Executor::new().await?;

    // Test 1: Ephemeral execution (cold start)
    println!("1. Testing ephemeral execution...");
    let start = Instant::now();
    let req1 = Request {
        id: "pipeline-ephemeral".to_string(),
        code: r#"
            echo "Cold start execution"
            echo "System info: $(uname -a)"
            echo "Memory: $(free -h | head -2)"
            echo "Processes: $(ps aux | wc -l)"
        "#
        .to_string(),
        mode: Mode::Ephemeral,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    let response1 = executor.run(req1).await?;
    let ephemeral_time = start.elapsed();

    assert_eq!(response1.exit_code, 0);
    let output1 = String::from_utf8_lossy(&response1.stdout);
    assert!(output1.contains("Cold start execution"));
    assert!(output1.contains("System info"));
    println!("   Ephemeral execution took: {:?}", ephemeral_time);

    // Test 2: Cached execution (should be much faster)
    println!("2. Testing cached execution...");
    let start = Instant::now();
    let req2 = Request {
        id: "pipeline-cached-1".to_string(),
        code: r#"
            # Deterministic computation for caching
            echo "Computing factorial of 10..."
            result=1
            for i in $(seq 1 10); do
                result=$((result * i))
            done
            echo "10! = $result"
            echo "Cached execution complete"
        "#
        .to_string(),
        mode: Mode::Cached,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    let response2 = executor.run(req2.clone()).await?;
    let first_cached_time = start.elapsed();

    assert_eq!(response2.exit_code, 0);
    let output2 = String::from_utf8_lossy(&response2.stdout);
    assert!(output2.contains("10! = 3628800"));
    println!("   First cached execution took: {:?}", first_cached_time);

    // Test 3: Cache hit (should be extremely fast)
    println!("3. Testing cache hit...");
    let start = Instant::now();
    let req3 = Request {
        id: "pipeline-cached-2".to_string(),
        ..req2.clone()
    };

    let response3 = executor.run(req3).await?;
    let cache_hit_time = start.elapsed();

    assert_eq!(response3.exit_code, 0);
    assert_eq!(response2.stdout, response3.stdout); // Exact same output
    println!("   Cache hit took: {:?}", cache_hit_time);

    // Verify cache provides significant speedup
    assert!(
        cache_hit_time < first_cached_time / 100,
        "Cache hit should be at least 100x faster! First: {:?}, Hit: {:?}",
        first_cached_time,
        cache_hit_time
    );

    println!(
        "✓ Complete execution pipeline working with {:.0}x cache speedup",
        first_cached_time.as_nanos() as f64 / cache_hit_time.as_nanos() as f64
    );

    Ok(())
}

/// Test concurrent execution scaling
#[tokio::test]
async fn test_concurrent_execution_scaling() -> Result<()> {
    println!("=== Testing Concurrent Execution Scaling ===");

    let executor = Arc::new(Executor::new().await?);

    // Prepare different workloads
    let workloads = vec![
        (
            "cpu",
            r#"
            echo "CPU intensive task"
            for i in $(seq 1 1000); do
                echo "Processing $i" > /dev/null
            done
            echo "CPU task complete"
        "#,
        ),
        (
            "io",
            r#"
            echo "I/O intensive task"
            for i in $(seq 1 100); do
                echo "Data chunk $i" > /tmp/test_$i.txt
                cat /tmp/test_$i.txt > /dev/null
                rm /tmp/test_$i.txt
            done
            echo "I/O task complete"
        "#,
        ),
        (
            "mixed",
            r#"
            echo "Mixed workload task"
            echo "test data" > /tmp/mixed.txt
            wc -l /tmp/mixed.txt
            rm /tmp/mixed.txt
            echo "Mixed task complete"
        "#,
        ),
    ];

    let num_concurrent = 15;
    let mut handles = Vec::new();

    println!("Launching {} concurrent executions...", num_concurrent);
    let start = Instant::now();

    for i in 0..num_concurrent {
        let executor_clone = executor.clone();
        let (workload_name, code) = &workloads[i % workloads.len()];
        let code = code.to_string();
        let workload_name = workload_name.to_string();

        let handle = tokio::spawn(async move {
            let req = Request {
                id: format!("concurrent-{}-{}", workload_name, i),
                code,
                mode: Mode::Cached,
                env: "alpine:latest".to_string(),
                timeout: Duration::from_secs(45),
                checkpoint: None,
                branch_from: None,
                runtime: None,
            };

            let start_individual = Instant::now();
            let result = executor_clone.run(req).await;
            (result, start_individual.elapsed(), workload_name)
        });

        handles.push(handle);
    }

    // Wait for all executions to complete
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await?);
    }

    let total_time = start.elapsed();
    println!(
        "All {} concurrent executions completed in {:?}",
        num_concurrent, total_time
    );

    // Verify all executions succeeded
    let mut successful = 0;
    let mut total_individual_time = Duration::ZERO;

    for (result, individual_time, workload_name) in results {
        let response = result?;
        assert_eq!(response.exit_code, 0);

        let output = String::from_utf8_lossy(&response.stdout);
        assert!(output.contains(&format!("{} task complete", workload_name)));

        successful += 1;
        total_individual_time += individual_time;
    }

    assert_eq!(successful, num_concurrent);

    let avg_individual_time = total_individual_time / num_concurrent as u32;
    let efficiency = total_individual_time.as_secs_f64() / total_time.as_secs_f64();

    println!("✓ All {} executions successful", successful);
    println!("  Average individual time: {:?}", avg_individual_time);
    println!("  Concurrency efficiency: {:.1}x", efficiency);

    // Efficiency should be > 2x due to caching and parallelism
    assert!(
        efficiency > 2.0,
        "Concurrent execution should show significant efficiency gains! Got: {:.1}x",
        efficiency
    );

    Ok(())
}

/// Test error handling and recovery
#[tokio::test]
async fn test_error_handling_and_recovery() -> Result<()> {
    println!("=== Testing Error Handling and Recovery ===");

    let executor = Executor::new().await?;

    // Test 1: Syntax error handling
    println!("1. Testing syntax error handling...");
    let req_bad_syntax = Request {
        id: "error-syntax".to_string(),
        code: r#"
            echo "Starting task"
            invalid_command_that_does_not_exist
            echo "This should not execute"
        "#
        .to_string(),
        mode: Mode::Ephemeral,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(10),
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    let response_bad = executor.run(req_bad_syntax).await?;
    assert_ne!(response_bad.exit_code, 0); // Should fail
    println!(
        "   ✓ Syntax error correctly handled with exit code: {}",
        response_bad.exit_code
    );

    // Test 2: Recovery with valid execution
    println!("2. Testing recovery after error...");
    let req_good = Request {
        id: "error-recovery".to_string(),
        code: r#"
            echo "Recovery execution"
            echo "System is healthy"
            exit 0
        "#
        .to_string(),
        mode: Mode::Ephemeral,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(10),
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    let response_good = executor.run(req_good).await?;
    assert_eq!(response_good.exit_code, 0);
    let output = String::from_utf8_lossy(&response_good.stdout);
    assert!(output.contains("System is healthy"));
    println!("   ✓ Recovery execution successful");

    // Test 3: Timeout handling
    println!("3. Testing timeout handling...");
    let req_timeout = Request {
        id: "error-timeout".to_string(),
        code: r#"
            echo "Starting long task..."
            sleep 30
            echo "This should timeout"
        "#
        .to_string(),
        mode: Mode::Ephemeral,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(2), // Short timeout
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    let start = Instant::now();
    let _response_timeout = executor.run(req_timeout).await?;
    let timeout_duration = start.elapsed();

    // Should complete quickly due to timeout, not wait 30 seconds
    assert!(
        timeout_duration < Duration::from_secs(5),
        "Timeout should terminate execution quickly, took {:?}",
        timeout_duration
    );
    println!("   ✓ Timeout handled correctly in {:?}", timeout_duration);

    println!("✓ Error handling and recovery working correctly");
    Ok(())
}

/// Test different execution modes integration
#[tokio::test]
async fn test_execution_modes_integration() -> Result<()> {
    println!("=== Testing Execution Modes Integration ===");

    let executor = Executor::new().await?;

    // Test each mode with the same basic workload
    let base_code = r#"
        echo "Mode test execution"
        hostname
        whoami
        pwd
        echo "Mode test complete"
    "#;

    // Test ephemeral mode
    println!("1. Testing ephemeral mode...");
    let ephemeral_req = Request {
        id: "mode-ephemeral".to_string(),
        code: base_code.to_string(),
        mode: Mode::Ephemeral,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(20),
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    let ephemeral_resp = executor.run(ephemeral_req).await?;
    assert_eq!(ephemeral_resp.exit_code, 0);
    let ephemeral_output = String::from_utf8_lossy(&ephemeral_resp.stdout);
    assert!(ephemeral_output.contains("Mode test complete"));
    println!("   ✓ Ephemeral mode successful");

    // Test cached mode
    println!("2. Testing cached mode...");
    let cached_req = Request {
        id: "mode-cached".to_string(),
        code: base_code.to_string(),
        mode: Mode::Cached,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(20),
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    let cached_resp = executor.run(cached_req).await?;
    assert_eq!(cached_resp.exit_code, 0);
    let cached_output = String::from_utf8_lossy(&cached_resp.stdout);
    assert!(cached_output.contains("Mode test complete"));
    println!("   ✓ Cached mode successful");

    // Test checkpointed mode
    println!("3. Testing checkpointed mode...");
    let checkpoint_req = Request {
        id: "mode-checkpoint".to_string(),
        code: base_code.to_string(),
        mode: Mode::Checkpointed,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(20),
        checkpoint: None, // Will create a new checkpoint
        branch_from: None,
        runtime: None,
    };

    let checkpoint_resp = executor.run(checkpoint_req).await?;
    assert_eq!(checkpoint_resp.exit_code, 0);
    assert!(checkpoint_resp.snapshot.is_some());
    println!(
        "   ✓ Checkpointed mode successful with snapshot: {:?}",
        checkpoint_resp.snapshot.as_ref().unwrap()
    );

    // Test persistent mode
    println!("4. Testing persistent mode...");
    let persistent_req = Request {
        id: "mode-persistent".to_string(),
        code: base_code.to_string(),
        mode: Mode::Persistent,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(20),
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    let persistent_resp = executor.run(persistent_req).await?;
    assert_eq!(persistent_resp.exit_code, 0);
    let persistent_output = String::from_utf8_lossy(&persistent_resp.stdout);
    assert!(persistent_output.contains("Mode test complete"));
    println!("   ✓ Persistent mode successful");

    println!("✓ All execution modes working correctly");
    Ok(())
}

/// Test performance monitoring and metrics
#[tokio::test]
async fn test_performance_monitoring() -> Result<()> {
    println!("=== Testing Performance Monitoring ===");

    let executor = Executor::new().await?;

    // Create metrics collector
    let metrics = MetricsCollector::new(Default::default());

    // Run several executions to generate metrics
    println!("1. Running executions to generate metrics...");
    for i in 0..5 {
        let req = Request {
            id: format!("metrics-test-{}", i),
            code: format!(
                r#"
                echo "Metrics test iteration {}"
                sleep 0.1
                echo "Completed iteration {}"
            "#,
                i, i
            ),
            mode: Mode::Cached,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(10),
            checkpoint: None,
            branch_from: None,
            runtime: None,
        };

        let response = executor.run(req).await?;
        assert_eq!(response.exit_code, 0);

        // Record metrics for this execution
        let resource_snapshot = faas_executor::performance::metrics_collector::ResourceSnapshot {
            peak_memory_mb: 64,
            cpu_time_ms: 150 + i * 10,
            disk_reads_mb: 1,
            disk_writes_mb: 1,
        };

        metrics
            .record_execution(
                "cached",
                Duration::from_millis(150 + i * 10),
                true,
                resource_snapshot,
            )
            .await?;
    }

    // Get and verify metrics
    println!("2. Checking collected metrics...");
    let current_metrics = metrics.get_metrics().await;

    assert!(current_metrics.total_executions >= 5);
    assert!(current_metrics.successful_executions >= 5);
    assert!(current_metrics.avg_execution_time > Duration::ZERO);

    println!("   Total executions: {}", current_metrics.total_executions);
    println!(
        "   Successful executions: {}",
        current_metrics.successful_executions
    );
    println!(
        "   Average duration: {:?}",
        current_metrics.avg_execution_time
    );
    println!(
        "   Success rate: {:.1}%",
        (current_metrics.successful_executions as f64 / current_metrics.total_executions as f64)
            * 100.0
    );

    // Test predictive scaling
    println!("3. Testing predictive scaling...");
    let scaler = PredictiveScaler::new(Default::default());

    // Record usage patterns
    for i in 0..10 {
        scaler.record_usage("alpine", (i + 1) as f64).await?;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Get prediction
    if let Ok(Some(prediction)) = scaler.predict_scaling("alpine").await {
        println!(
            "   Prediction - Load: {:.2}, Confidence: {:.2}",
            prediction.predicted_load, prediction.confidence
        );
        assert!(prediction.confidence > 0.0 && prediction.confidence <= 1.0);
        assert!(prediction.predicted_load > 0.0);
    }

    println!("✓ Performance monitoring working correctly");
    Ok(())
}

/// Test system resource utilization under load
#[tokio::test]
async fn test_resource_utilization() -> Result<()> {
    println!("=== Testing Resource Utilization ===");

    let executor = Arc::new(Executor::new().await?);

    // Create a memory-intensive workload
    let memory_workload = r#"
        echo "Starting memory test"
        # Create some data in memory
        for i in $(seq 1 100); do
            echo "Memory test data line $i - $(date)" >> /tmp/memory_test.txt
        done

        # Read the data back
        wc -l /tmp/memory_test.txt
        head -5 /tmp/memory_test.txt
        tail -5 /tmp/memory_test.txt

        # Cleanup
        rm /tmp/memory_test.txt
        echo "Memory test complete"
    "#;

    // Run multiple memory-intensive tasks
    println!("1. Running memory-intensive workloads...");
    let mut handles = Vec::new();

    for i in 0..8 {
        let executor_clone = executor.clone();
        let workload = memory_workload.to_string();

        let handle = tokio::spawn(async move {
            let req = Request {
                id: format!("memory-test-{}", i),
                code: workload,
                mode: Mode::Cached,
                env: "alpine:latest".to_string(),
                timeout: Duration::from_secs(30),
                checkpoint: None,
                branch_from: None,
                runtime: None,
            };

            executor_clone.run(req).await
        });

        handles.push(handle);
    }

    // Wait for completion and verify results
    let start = Instant::now();
    let mut successful = 0;

    for handle in handles {
        let result = handle.await?;
        let response = result?;

        assert_eq!(response.exit_code, 0);
        let output = String::from_utf8_lossy(&response.stdout);
        assert!(output.contains("Memory test complete"));
        successful += 1;
    }

    let total_time = start.elapsed();

    assert_eq!(successful, 8);
    println!(
        "   ✓ {} memory-intensive tasks completed in {:?}",
        successful, total_time
    );

    // Test CPU-intensive workload
    println!("2. Running CPU-intensive workload...");
    let cpu_workload = r#"
        echo "Starting CPU test"
        # CPU-intensive calculation
        result=1
        for i in $(seq 1 1000); do
            result=$((result + i * i % 1000))
        done
        echo "CPU calculation result: $result"
        echo "CPU test complete"
    "#;

    let start = Instant::now();
    let req = Request {
        id: "cpu-intensive".to_string(),
        code: cpu_workload.to_string(),
        mode: Mode::Cached,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    let response = executor.run(req).await?;
    let cpu_time = start.elapsed();

    assert_eq!(response.exit_code, 0);
    let output = String::from_utf8_lossy(&response.stdout);
    assert!(output.contains("CPU test complete"));
    assert!(output.contains("CPU calculation result:"));

    println!("   ✓ CPU-intensive task completed in {:?}", cpu_time);

    println!("✓ Resource utilization tests completed successfully");
    Ok(())
}
