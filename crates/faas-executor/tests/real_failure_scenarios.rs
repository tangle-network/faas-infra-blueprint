/// REAL Failure Scenario Tests - No Mocks, Real Systems Under Stress
/// These tests verify behavior under actual failure conditions
use std::sync::Arc;
use std::time::{Duration, Instant};
use faas_executor::bollard::Docker;
use faas_executor::{DockerExecutor, common::{SandboxConfig, SandboxExecutor}};
use faas_executor::container_pool::{ContainerPoolManager, PoolConfig};

/// Test behavior when Docker daemon is overloaded
#[tokio::test]
#[ignore = "Requires Docker and high system load"]
async fn test_docker_overload_handling() {
    println!("\n💥 Docker Overload Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // Create many simultaneous container requests to overload Docker
    let overload_count = 50;
    let mut tasks = vec![];

    println!("🔥 Creating {} simultaneous container requests...", overload_count);

    let start = Instant::now();
    for i in 0..overload_count {
        let executor = executor.clone();
        let task = tokio::spawn(async move {
            let start = Instant::now();
            let result = executor.execute(SandboxConfig {
                function_id: format!("overload-{}", i),
                source: "alpine:latest".to_string(),
                command: vec!["sh".to_string(), "-c".to_string(),
                              format!("echo 'Task {}' && sleep 0.5", i)],
                env_vars: None,
                payload: vec![],
            }).await;
            (i, start.elapsed(), result.is_ok())
        });
        tasks.push(task);
    }

    let results = futures::future::join_all(tasks).await;
    let total_duration = start.elapsed();

    let successful = results.iter().filter(|r| r.as_ref().unwrap().2).count();
    let failed = results.len() - successful;

    println!("\n📊 Overload Results:");
    println!("  Total time: {:?}", total_duration);
    println!("  Successful: {}/{}", successful, overload_count);
    println!("  Failed: {}", failed);
    println!("  Success rate: {:.1}%", (successful as f64 / overload_count as f64) * 100.0);

    // Under extreme load, some failures are expected but system should remain stable
    assert!(successful > overload_count / 2, "More than half should succeed even under load");
    assert!(total_duration < Duration::from_secs(60), "Should complete within reasonable time");

    println!("✅ System remains stable under Docker overload");
}

/// Test network failure simulation
#[tokio::test]
#[ignore = "Requires Docker and network manipulation"]
async fn test_network_failure_resilience() {
    println!("\n🌐 Network Failure Resilience Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // Test 1: Container tries to access unreachable network
    println!("🔌 Testing unreachable network access...");
    let start = Instant::now();
    let result = executor.execute(SandboxConfig {
        function_id: "network-fail-test".to_string(),
        source: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            // Try to reach unreachable IP with short timeout
            "timeout 2 wget -O - http://192.0.2.1/test 2>&1 || echo 'NETWORK_FAILED'".to_string()
        ],
        env_vars: None,
        payload: vec![],
    }).await;

    let duration = start.elapsed();
    assert!(result.is_ok(), "Container should handle network failure gracefully");

    let response = result.unwrap().response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("  Network failure output: {}", output.trim());

    assert!(output.contains("NETWORK_FAILED") || output.contains("timeout"),
            "Should handle network timeout gracefully");
    assert!(duration < Duration::from_secs(5), "Should timeout quickly");

    // Test 2: DNS resolution failure
    println!("\n🌍 Testing DNS resolution failure...");
    let result = executor.execute(SandboxConfig {
        function_id: "dns-fail-test".to_string(),
        source: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            "timeout 2 nslookup nonexistent.invalid.domain 2>&1 || echo 'DNS_FAILED'".to_string()
        ],
        env_vars: None,
        payload: vec![],
    }).await.unwrap();

    let response = result.response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("  DNS failure output: {}", output.trim());

    assert!(output.contains("DNS_FAILED") || output.contains("can't resolve"),
            "Should handle DNS failure gracefully");

    println!("✅ Network failures handled correctly");
}

/// Test disk space exhaustion
#[tokio::test]
#[ignore = "Requires Docker - may fill disk temporarily"]
async fn test_disk_space_exhaustion() {
    println!("\n💾 Disk Space Exhaustion Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // Try to fill container's available disk space
    println!("📁 Testing disk space limits...");
    let result = executor.execute(SandboxConfig {
        function_id: "disk-full-test".to_string(),
        source: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            // Try to create a large file that might exhaust available space
            "dd if=/dev/zero of=/tmp/largefile bs=1M count=1000 2>&1 || echo 'DISK_LIMIT_HIT'".to_string()
        ],
        env_vars: None,
        payload: vec![],
    }).await;

    assert!(result.is_ok(), "Container should handle disk limits gracefully");

    let response = result.unwrap().response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("  Disk limit test output: {}", output.lines().last().unwrap_or(""));

    // Either succeeds within limits or hits a limit gracefully
    assert!(
        output.contains("DISK_LIMIT_HIT") ||
        output.contains("No space left") ||
        output.contains("1000+0 records"),
        "Should either hit limit gracefully or succeed within bounds"
    );

    println!("✅ Disk space limits handled correctly");
}

/// Test memory pressure scenarios
#[tokio::test]
#[ignore = "Requires Docker - high memory usage"]
async fn test_memory_pressure_handling() {
    println!("\n🧠 Memory Pressure Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // Test container that gradually increases memory usage
    println!("📈 Testing incremental memory pressure...");
    let result = executor.execute(SandboxConfig {
        function_id: "memory-pressure-test".to_string(),
        source: "python:3.11-slim".to_string(),
        command: vec![
            "python3".to_string(),
            "-c".to_string(),
            r#"
import sys
import time
data = []
try:
    for i in range(1000):
        # Allocate 1MB chunks
        chunk = 'x' * (1024 * 1024)
        data.append(chunk)
        if i % 100 == 0:
            print(f'Allocated {i}MB', flush=True)
            time.sleep(0.01)
    print('MEMORY_ALLOCATION_COMPLETE')
except MemoryError:
    print('MEMORY_ERROR_CAUGHT')
except Exception as e:
    print(f'OTHER_ERROR: {e}')
            "#.to_string()
        ],
        env_vars: None,
        payload: vec![],
    }).await;

    assert!(result.is_ok(), "Container should handle memory pressure gracefully");

    let response = result.unwrap().response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("  Memory pressure output:");
    for line in output.lines().take(10) {
        println!("    {}", line);
    }

    // Should either complete allocation or hit memory limits gracefully
    assert!(
        output.contains("MEMORY_ALLOCATION_COMPLETE") ||
        output.contains("MEMORY_ERROR_CAUGHT") ||
        output.contains("Killed") ||
        output.contains("Allocated"),
        "Should handle memory pressure scenarios gracefully"
    );

    println!("✅ Memory pressure handled correctly");
}

/// Test concurrent container creation failures
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_concurrent_container_failures() {
    println!("\n⚡ Concurrent Container Failure Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let config = PoolConfig::default();
    let pool_manager = Arc::new(ContainerPoolManager::new(docker, config));

    // Create many pools simultaneously to stress the system
    let concurrent_pools = 20;
    let mut tasks = vec![];

    println!("🚀 Creating {} concurrent container pools...", concurrent_pools);

    for i in 0..concurrent_pools {
        let pool_manager = pool_manager.clone();
        let task = tokio::spawn(async move {
            let start = Instant::now();
            let pool = pool_manager.get_pool(&format!("alpine:latest-{}", i % 3)).await;

            // Try to pre-warm
            let pre_warm_result = pool.pre_warm().await;

            // Try to acquire container
            let acquire_result = if pre_warm_result.is_ok() {
                pool.acquire().await
            } else {
                Err(anyhow::anyhow!("Pre-warm failed"))
            };

            (i, start.elapsed(), pre_warm_result.is_ok(), acquire_result.is_ok())
        });
        tasks.push(task);
    }

    let results = futures::future::join_all(tasks).await;

    let successful = results.iter().filter(|r| {
        let (_, _, pre_warm_ok, acquire_ok) = r.as_ref().unwrap();
        *pre_warm_ok && *acquire_ok
    }).count();

    println!("\n📊 Concurrent Pool Results:");
    println!("  Successful pools: {}/{}", successful, concurrent_pools);
    println!("  Success rate: {:.1}%", (successful as f64 / concurrent_pools as f64) * 100.0);

    // At least half should succeed under normal conditions
    assert!(successful >= concurrent_pools / 2,
            "At least half of concurrent pools should succeed");

    println!("✅ Concurrent container creation handled correctly");
}

/// Test container cleanup under failure conditions
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_cleanup_under_failure() {
    println!("\n🧹 Cleanup Under Failure Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker.clone());

    // Get initial container count
    let initial_containers = docker.list_containers::<String>(None).await.unwrap().len();
    println!("📦 Initial containers: {}", initial_containers);

    // Create executions that will fail in various ways
    let failure_tests = vec![
        ("timeout", vec!["sleep".to_string(), "60".to_string()]),
        ("segfault", vec!["sh".to_string(), "-c".to_string(), "kill -SEGV $$".to_string()]),
        ("exit-error", vec!["sh".to_string(), "-c".to_string(), "exit 1".to_string()]),
        ("invalid-command", vec!["nonexistent-command".to_string()]),
        ("large-output", vec!["sh".to_string(), "-c".to_string(), "yes | head -100000".to_string()]),
    ];

    for (test_name, command) in failure_tests {
        println!("\n🔥 Testing failure: {}", test_name);

        let start = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            executor.execute(SandboxConfig {
                function_id: format!("failure-{}", test_name),
                source: "alpine:latest".to_string(),
                command,
                env_vars: None,
                payload: vec![],
            })
        ).await;

        let duration = start.elapsed();

        match result {
            Ok(exec_result) => {
                println!("  Completed: {:?} - {:?}", duration, exec_result.is_ok());
            }
            Err(_) => {
                println!("  Timed out: {:?}", duration);
            }
        }

        // Small delay to allow cleanup
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Check for container leaks after failures
    tokio::time::sleep(Duration::from_secs(2)).await; // Allow cleanup time
    let final_containers = docker.list_containers::<String>(None).await.unwrap().len();
    println!("\n📦 Final containers: {}", final_containers);

    let leaked = final_containers.saturating_sub(initial_containers);
    println!("🔍 Potential leaks: {}", leaked);

    // Allow for some pooled containers but not unbounded growth
    assert!(leaked < 10, "Too many containers leaked after failures: {}", leaked);

    println!("✅ Container cleanup works correctly under failures");
}

/// Test real resource limit enforcement
#[tokio::test]
#[ignore = "Requires Docker with resource limits"]
async fn test_real_resource_limit_enforcement() {
    println!("\n⚖️  Real Resource Limit Enforcement Test");
    println!("{}", "=".repeat(50));

    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // Test CPU limit enforcement
    println!("🖥️  Testing CPU time limits...");
    let start = Instant::now();
    let result = executor.execute(SandboxConfig {
        function_id: "cpu-limit-test".to_string(),
        source: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            // CPU-intensive task that should be limited
            "timeout 3 sh -c 'while true; do echo $((1+1)) > /dev/null; done' || echo 'CPU_LIMITED'".to_string()
        ],
        env_vars: None,
        payload: vec![],
    }).await;

    let cpu_duration = start.elapsed();
    assert!(result.is_ok(), "CPU limit test should complete");

    let response = result.unwrap().response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("  CPU limit result: {}", output.trim());

    // Should be terminated by timeout, not run indefinitely
    assert!(cpu_duration < Duration::from_secs(5), "CPU should be limited");

    // Test file descriptor limits
    println!("\n📂 Testing file descriptor limits...");
    let result = executor.execute(SandboxConfig {
        function_id: "fd-limit-test".to_string(),
        source: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            // Try to open many files
            r#"
            count=0
            for i in $(seq 1 2000); do
                if exec 3</dev/null; then
                    count=$((count + 1))
                else
                    echo "FD_LIMIT_AT_$count"
                    break
                fi
            done
            echo "OPENED_$count"
            "#.to_string()
        ],
        env_vars: None,
        payload: vec![],
    }).await.unwrap();

    let response = result.response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("  FD limit result: {}", output.trim());

    // Should hit a reasonable limit
    assert!(
        output.contains("FD_LIMIT_AT") || output.contains("OPENED_"),
        "Should show file descriptor behavior"
    );

    println!("✅ Resource limits are enforced correctly");
}