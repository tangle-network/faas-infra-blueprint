use faas_executor::platform::{Executor, Mode, Request, Response};
use std::time::Duration;
use tokio;

/// Integration tests for the multi-mode execution platform
///
/// These tests validate all 5 execution modes work correctly
/// and achieve the performance targets specified in the platform spec.

#[tokio::test]
async fn test_ephemeral_mode_performance() {
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping test: Executor initialization failed (likely missing Firecracker)");
            return;
        }
    };

    let req = Request {
        id: "ephemeral-test".to_string(),
        code: "echo 'Ephemeral execution test'".to_string(),
        mode: Mode::Ephemeral,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
    };

    let start = std::time::Instant::now();
    let result = executor.run(req).await.unwrap();
    let duration = start.elapsed();

    assert_eq!(result.exit_code, 0);
    assert!(duration < Duration::from_millis(100), "Ephemeral mode too slow: {:?}", duration);
    assert!(result.stdout.len() > 0);
    println!("✅ Ephemeral mode: {:?}", duration);
}

#[tokio::test]
async fn test_cached_mode_performance() {
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping test: Executor initialization failed");
            return;
        }
    };

    let req = Request {
        id: "cached-test".to_string(),
        code: "echo 'Cached execution test'".to_string(),
        mode: Mode::Cached,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
    };

    let start = std::time::Instant::now();
    let result = executor.run(req).await.unwrap();
    let duration = start.elapsed();

    assert_eq!(result.exit_code, 0);
    assert!(duration < Duration::from_millis(200), "Cached mode too slow: {:?}", duration);
    println!("✅ Cached mode: {:?}", duration);
}

#[tokio::test]
async fn test_checkpointed_mode_create_and_restore() {
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping test: Executor initialization failed");
            return;
        }
    };

    // Create checkpoint
    let create_req = Request {
        id: "checkpoint-create-test".to_string(),
        code: "echo 'Creating checkpoint'".to_string(),
        mode: Mode::Checkpointed,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
    };

    let start = std::time::Instant::now();
    let create_result = executor.run(create_req).await.unwrap();
    let create_duration = start.elapsed();

    assert_eq!(create_result.exit_code, 0);
    assert!(create_result.snapshot.is_some());
    assert!(create_duration < Duration::from_millis(300), "Checkpoint creation too slow: {:?}", create_duration);
    println!("✅ Checkpoint create: {:?}", create_duration);

    // Restore from checkpoint
    let restore_req = Request {
        id: "checkpoint-restore-test".to_string(),
        code: "echo 'Restoring from checkpoint'".to_string(),
        mode: Mode::Checkpointed,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: create_result.snapshot,
        branch_from: None,
    };

    let start = std::time::Instant::now();
    let restore_result = executor.run(restore_req).await.unwrap();
    let restore_duration = start.elapsed();

    assert_eq!(restore_result.exit_code, 0);
    assert!(restore_duration < Duration::from_millis(350), "Checkpoint restore too slow: {:?}", restore_duration);
    println!("✅ Checkpoint restore: {:?}", restore_duration);
}

#[tokio::test]
async fn test_branched_mode_ai_agent_pattern() {
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping test: Executor initialization failed");
            return;
        }
    };

    // Create parent execution that can be branched from
    let parent_req = Request {
        id: "branch-parent".to_string(),
        code: "echo 'Parent execution state'".to_string(),
        mode: Mode::Checkpointed,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
    };

    let parent_result = executor.run(parent_req).await.unwrap();
    assert!(parent_result.snapshot.is_some());

    // Create multiple branches (AI agent exploration pattern)
    let mut branch_tasks = Vec::new();
    for i in 0..3 {
        let branch_req = Request {
            id: format!("branch-{}", i),
            code: format!("echo 'Branch {} exploring'", i),
            mode: Mode::Branched,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: parent_result.snapshot.clone(),
        };

        branch_tasks.push(executor.run(branch_req));
    }

    let start = std::time::Instant::now();
    let results = futures::future::join_all(branch_tasks).await;
    let total_duration = start.elapsed();

    // Verify all branches succeeded
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "Branch {} failed: {:?}", i, result);
        assert_eq!(result.as_ref().unwrap().exit_code, 0);
    }

    // Parallel branching should be fast
    assert!(total_duration < Duration::from_millis(200), "Parallel branching too slow: {:?}", total_duration);
    println!("✅ Parallel branching (3 branches): {:?}", total_duration);
}

#[tokio::test]
async fn test_persistent_mode() {
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping test: Executor initialization failed");
            return;
        }
    };

    let req = Request {
        id: "persistent-test".to_string(),
        code: "echo 'Persistent execution test'".to_string(),
        mode: Mode::Persistent,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
    };

    let start = std::time::Instant::now();
    let result = executor.run(req).await.unwrap();
    let duration = start.elapsed();

    assert_eq!(result.exit_code, 0);
    // Persistent mode may be slower due to VM overhead
    assert!(duration < Duration::from_secs(1), "Persistent mode too slow: {:?}", duration);
    println!("✅ Persistent mode: {:?}", duration);
}

#[tokio::test]
async fn test_concurrent_mixed_modes() {
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping test: Executor initialization failed");
            return;
        }
    };

    // Test concurrent execution of different modes
    let mut tasks = Vec::new();

    // Ephemeral tasks
    for i in 0..2 {
        let req = Request {
            id: format!("concurrent-ephemeral-{}", i),
            code: format!("echo 'Concurrent ephemeral {}'", i),
            mode: Mode::Ephemeral,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };
        tasks.push(executor.run(req));
    }

    // Cached tasks
    for i in 0..2 {
        let req = Request {
            id: format!("concurrent-cached-{}", i),
            code: format!("echo 'Concurrent cached {}'", i),
            mode: Mode::Cached,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };
        tasks.push(executor.run(req));
    }

    let start = std::time::Instant::now();
    let results = futures::future::join_all(tasks).await;
    let total_duration = start.elapsed();

    // Verify all executions succeeded
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "Concurrent execution {} failed: {:?}", i, result);
        assert_eq!(result.as_ref().unwrap().exit_code, 0);
    }

    // Concurrent execution should scale well
    assert!(total_duration < Duration::from_secs(2), "Concurrent execution too slow: {:?}", total_duration);
    println!("✅ Concurrent mixed modes (4 tasks): {:?}", total_duration);
}

#[tokio::test]
async fn test_stress_ephemeral_burst() {
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping test: Executor initialization failed");
            return;
        }
    };

    // Burst of 10 ephemeral executions
    let mut tasks = Vec::new();
    for i in 0..10 {
        let req = Request {
            id: format!("burst-{}", i),
            code: format!("echo 'Burst execution {}'", i),
            mode: Mode::Ephemeral,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };
        tasks.push(executor.run(req));
    }

    let start = std::time::Instant::now();
    let results = futures::future::join_all(tasks).await;
    let total_duration = start.elapsed();

    // Verify all executions succeeded
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "Burst execution {} failed: {:?}", i, result);
        assert_eq!(result.as_ref().unwrap().exit_code, 0);
    }

    // Burst should complete reasonably fast
    assert!(total_duration < Duration::from_secs(5), "Burst execution too slow: {:?}", total_duration);
    println!("✅ Burst test (10 executions): {:?}", total_duration);
    println!("   Average per execution: {:?}", total_duration / 10);
}

#[tokio::test]
async fn test_error_handling() {
    let executor = match Executor::new().await {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping test: Executor initialization failed");
            return;
        }
    };

    // Test with invalid branch_from (should fail gracefully)
    let req = Request {
        id: "error-test".to_string(),
        code: "echo 'This should fail'".to_string(),
        mode: Mode::Branched,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: Some("non-existent-snapshot".to_string()),
    };

    let result = executor.run(req).await;
    assert!(result.is_err(), "Expected error for invalid branch_from");
    println!("✅ Error handling works correctly");
}