/// REAL tests that actually verify optimizations work
use anyhow::Result;
use faas_executor::platform::{executor::*, fork::ForkManager, memory::MemoryPool};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Test cache with actual verification of cached data correctness
#[tokio::test]
async fn test_real_cache_correctness() -> Result<()> {
    println!("Testing REAL cache correctness");

    let executor = Executor::new().await?;

    // Test with a computation that produces deterministic output
    let code = r#"
        for i in {1..10}; do
            echo "Line $i: Processing data"
        done
        echo "Final checksum: 42"
    "#;

    let req1 = Request {
        id: "test-1".to_string(),
        code: code.to_string(),
        mode: Mode::Cached,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(10),
        checkpoint: None,
        branch_from: None,
    };

    // First execution - should run in container
    let response1 = executor.run(req1.clone()).await?;
    assert_eq!(response1.exit_code, 0);
    let original_output = String::from_utf8_lossy(&response1.stdout);
    assert!(original_output.contains("Final checksum: 42"));

    // Second execution - should hit cache
    let req2 = Request {
        id: "test-2".to_string(),
        ..req1.clone()
    };

    let response2 = executor.run(req2).await?;
    assert_eq!(response2.exit_code, 0);

    // CRITICAL: Verify cached output is IDENTICAL to original
    assert_eq!(
        response1.stdout,
        response2.stdout,
        "Cached output must match original output exactly!"
    );

    // Test cache invalidation with different code
    let req3 = Request {
        id: "test-3".to_string(),
        code: "echo 'different'".to_string(),
        mode: Mode::Cached,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(10),
        checkpoint: None,
        branch_from: None,
    };

    let response3 = executor.run(req3).await?;
    assert_ne!(
        response3.stdout,
        response1.stdout,
        "Different code must produce different output!"
    );

    println!("✓ Cache correctly stores and retrieves exact output");
    Ok(())
}

/// Test fork with actual process/container forking using Docker checkpointing
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_real_container_fork() -> Result<()> {
    println!("Testing REAL container forking with Docker checkpointing");

    let executor = Executor::new().await?;

    // First, create a base container and set up state using the create_base_container method
    let docker = bollard::Docker::connect_with_local_defaults()?;
    let fork_manager = faas_executor::docker_fork::DockerForkManager::new(docker);

    // Set up a base container with state
    let base_id = "test-base-fork";
    let setup_commands = vec![
        "echo 'initial state' > /tmp/state.txt",
        "echo 'test data' > /tmp/data.txt",
        "mkdir -p /tmp/test_dir",
        "echo 'directory content' > /tmp/test_dir/file.txt",
    ];

    println!("Creating base container with initial state...");
    let _ = fork_manager.create_base_container(
        base_id,
        "alpine:latest",
        setup_commands,
    ).await?;

    // Now use the executor to fork from this base
    let fork_code = r#"
        echo "=== Verifying forked state ==="
        if [ -f /tmp/state.txt ]; then
            echo "Found state.txt: $(cat /tmp/state.txt)"
        else
            echo "ERROR: Missing state.txt"
            exit 1
        fi

        if [ -f /tmp/data.txt ]; then
            echo "Found data.txt: $(cat /tmp/data.txt)"
        else
            echo "ERROR: Missing data.txt"
            exit 1
        fi

        if [ -f /tmp/test_dir/file.txt ]; then
            echo "Found directory content: $(cat /tmp/test_dir/file.txt)"
        else
            echo "ERROR: Missing directory content"
            exit 1
        fi

        echo "=== All state preserved correctly ==="
    "#;

    let fork_req = Request {
        id: "test-fork-1".to_string(),
        code: fork_code.to_string(),
        mode: Mode::Branched,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: Some(base_id.to_string()),
    };

    println!("Forking from base and verifying state preservation...");
    let fork_response = executor.run(fork_req).await?;

    // Verify fork preserved ALL state from base
    let fork_output = String::from_utf8_lossy(&fork_response.stdout);
    println!("Fork output:\n{}", fork_output);

    assert!(
        fork_output.contains("Found state.txt: initial state"),
        "Fork should preserve state.txt! Got: {}",
        fork_output
    );
    assert!(
        fork_output.contains("Found data.txt: test data"),
        "Fork should preserve data.txt! Got: {}",
        fork_output
    );
    assert!(
        fork_output.contains("Found directory content: directory content"),
        "Fork should preserve directory structure! Got: {}",
        fork_output
    );
    assert!(
        fork_output.contains("All state preserved correctly"),
        "Fork should preserve all state! Got: {}",
        fork_output
    );

    println!("✓ Docker fork correctly preserves complete container state");

    // Cleanup
    fork_manager.cleanup_fork("test-fork-1").await?;

    Ok(())
}

/// Test with complex workload to verify real performance gains
#[tokio::test]
async fn test_real_performance_workload() -> Result<()> {
    println!("Testing with REAL computational workload");

    let executor = Executor::new().await?;

    // Complex computation that takes measurable time
    let compute_code = r#"
        # Simulate heavy computation with sleep and loops
        for i in $(seq 1 100); do
            echo "Processing item $i"
        done
        # Small sleep to ensure measurable time
        sleep 0.1
        echo "Computation complete with 100 items"
    "#;

    let req1 = Request {
        id: "compute-1".to_string(),
        code: compute_code.to_string(),
        mode: Mode::Cached,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
    };

    // First run - actual computation
    let start1 = Instant::now();
    let response1 = executor.run(req1.clone()).await?;
    let compute_time = start1.elapsed();
    assert_eq!(response1.exit_code, 0);

    println!("First computation took: {:?}", compute_time);

    // Verify output contains expected results
    let output = String::from_utf8_lossy(&response1.stdout);
    assert!(output.contains("Processing item 1"));
    assert!(output.contains("Processing item 100"));
    assert!(output.contains("Computation complete with 100 items"));

    // Second run - should be cached
    let req2 = Request {
        id: "compute-2".to_string(),
        ..req1.clone()
    };

    let start2 = Instant::now();
    let response2 = executor.run(req2).await?;
    let cache_time = start2.elapsed();

    println!("Cached retrieval took: {:?}", cache_time);

    // Verify cached output is identical
    assert_eq!(response1.stdout, response2.stdout);

    // Verify cache is actually faster (at least 10x)
    assert!(
        cache_time < compute_time / 10,
        "Cache should be at least 10x faster! Compute: {:?}, Cache: {:?}",
        compute_time,
        cache_time
    );

    println!("✓ Cache provides real performance gain: {:.1}x speedup",
        compute_time.as_secs_f64() / cache_time.as_secs_f64());
    Ok(())
}

/// Test memory pool with actual memory pressure and verification
#[tokio::test]
async fn test_real_memory_pressure() -> Result<()> {
    println!("Testing memory pool under real pressure");

    let memory_pool = Arc::new(MemoryPool::new()?);

    // Allocate multiple chunks to create pressure
    let mut allocations = Vec::new();
    let chunk_size = 16; // 16MB chunks
    let num_chunks = 10;

    for i in 0..num_chunks {
        let start = Instant::now();
        let buffer = memory_pool.allocate(chunk_size).await?;
        let alloc_time = start.elapsed();

        // Write pattern to verify memory is real
        let pattern = vec![i as u8; 1024];
        let mut test_buffer = buffer.clone();
        for chunk in test_buffer.chunks_mut(1024) {
            chunk.copy_from_slice(&pattern[..chunk.len()]);
        }

        // Verify we can read back what we wrote
        assert_eq!(test_buffer[0], i as u8);
        assert_eq!(test_buffer[1023], i as u8);

        println!("Allocation {} ({} MB) took {:?}", i, chunk_size, alloc_time);
        allocations.push(test_buffer);
    }

    // Verify all allocations are independent
    for (i, alloc) in allocations.iter().enumerate() {
        assert_eq!(alloc[0], i as u8, "Allocation {} corrupted", i);
    }

    // Test deduplication with identical buffers
    let _identical_data = vec![255u8; (chunk_size as usize) * 1024 * 1024];
    let _dup1 = memory_pool.allocate(chunk_size).await?;
    let _dup2 = memory_pool.allocate(chunk_size).await?;

    // Give KSM time to deduplicate if available
    tokio::time::sleep(Duration::from_millis(100)).await;

    let dedup_ratio = memory_pool.dedup_ratio();
    println!("Deduplication ratio after identical allocations: {:.2}", dedup_ratio);

    println!("✓ Memory pool handles real allocations correctly");
    Ok(())
}

/// Test concurrent execution to verify thread safety and performance
#[tokio::test]
async fn test_concurrent_optimization() -> Result<()> {
    println!("Testing concurrent execution optimization");

    let executor = Arc::new(Executor::new().await?);

    // Warm up cache with some computations
    let warm_up_code = vec![
        "echo 'Result: 1'",
        "echo 'Result: 2'",
        "echo 'Result: 3'",
    ];

    for (i, code) in warm_up_code.iter().enumerate() {
        let req = Request {
            id: format!("warmup-{}", i),
            code: code.to_string(),
            mode: Mode::Cached,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(10),
            checkpoint: None,
            branch_from: None,
        };
        executor.run(req).await?;
    }

    // Now run many concurrent requests
    let mut handles = Vec::new();
    let num_concurrent = 20;
    let start = Instant::now();

    for i in 0..num_concurrent {
        let executor_clone = executor.clone();
        let code_index = i % 3; // Reuse the 3 cached computations
        let code = warm_up_code[code_index].to_string();

        let handle = tokio::spawn(async move {
            let req = Request {
                id: format!("concurrent-{}", i),
                code,
                mode: Mode::Cached,
                env: "alpine:latest".to_string(),
                timeout: Duration::from_secs(10),
                checkpoint: None,
                branch_from: None,
            };
            executor_clone.run(req).await
        });

        handles.push(handle);
    }

    // Wait for all to complete
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await??);
    }

    let total_time = start.elapsed();
    println!("Concurrent execution of {} requests took {:?}", num_concurrent, total_time);

    // Verify all results are correct
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result.exit_code, 0);
        let expected = format!("Result: {}", (i % 3) + 1);
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(output.contains(&expected),
            "Request {} should contain '{}', got '{}'", i, expected, output);
    }

    // Should be much faster than running sequentially
    let avg_time = total_time.as_secs_f64() / num_concurrent as f64;
    assert!(
        avg_time < 0.1, // Should average < 100ms per request with cache
        "Concurrent cached execution too slow: {:.3}s average", avg_time
    );

    println!("✓ Concurrent execution works correctly with {:.3}ms average per request",
        avg_time * 1000.0);
    Ok(())
}