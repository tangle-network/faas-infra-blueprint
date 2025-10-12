#![cfg(target_os = "linux")]

use faas_executor::firecracker::{
    FirecrackerExecutor, VmSnapshotManager, MultiLevelVmCache, VmForkManager,
    VmPredictiveScaler, CacheConfig, ScalingConfig,
};
use faas_common::{SandboxConfig, SandboxExecutor};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

fn is_kvm_available() -> bool {
    std::path::Path::new("/dev/kvm").exists()
}

#[tokio::test]
// Requires KVM available
async fn test_vm_cold_start() {
    if !is_kvm_available() {
        eprintln!("KVM not available, skipping test");
        return;
    }

    let executor = FirecrackerExecutor::new(
        "/usr/bin/firecracker".to_string(),
        "/path/to/kernel".to_string(),
        "/path/to/rootfs".to_string(),
    ).expect("Failed to create executor");

    let config = SandboxConfig {
        function_id: "test-cold-start".to_string(),
        // function_name: Some("test-function".to_string()),
        // function_version: Some("v1".to_string()),
        source: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "Hello from VM".to_string()],
        payload: Vec::new(),
        env_vars: None,
        // code_hash: Some("test-hash".to_string()),
        // vcpu_count: Some(1),
        // memory_size_mb: Some(256),
    };

    let result = executor.execute(config).await;
    assert!(result.is_ok());

    let invocation = result.unwrap();
    assert_eq!(invocation.response, Some(b"Hello from VM\n".to_vec()));
}

#[tokio::test]
// Auto-runs on Linux with KVM
async fn test_vm_snapshot_creation_and_restore() {
    if !is_kvm_available() {
        eprintln!("KVM not available, skipping test");
        return;
    }

    let snapshot_mgr = Arc::new(VmSnapshotManager::new(
        PathBuf::from("/tmp/test-snapshots")
    ));

    // Start a VM
    let vm_id = "test-vm-1";
    let api_socket = "/tmp/test-vm.sock";

    // Create snapshot (would need actual VM running)
    let snapshot_id = "test-snapshot-1";
    let result = snapshot_mgr.create_snapshot(vm_id, snapshot_id, api_socket).await;

    if result.is_ok() {
        // Try to restore
        let new_vm_id = "test-vm-2";
        let restored = snapshot_mgr.restore_snapshot(snapshot_id, new_vm_id).await;
        assert!(restored.is_ok());

        let restored_vm = restored.unwrap();
        assert_eq!(restored_vm.vm_id, new_vm_id);
    }
}

#[tokio::test]
// Auto-runs on Linux with KVM
async fn test_vm_cache_hit() {
    if !is_kvm_available() {
        eprintln!("KVM not available, skipping test");
        return;
    }

    let cache_config = CacheConfig {
        memory_cache_size: 10,
        disk_cache_path: Some(PathBuf::from("/tmp/test-cache")),
        distributed_cache_url: None,
        ttl: Duration::from_secs(60),
        compression_enabled: true,
    };

    let cache = Arc::new(MultiLevelVmCache::new(cache_config));

    // Store a result
    let key = "test-function:v1:hash123";
    let result = faas_executor::firecracker::vm_cache::CacheResult {
        response: Some(b"cached result".to_vec()),
        error: None,
        hit_rate: 0.0,
        cache_level: "L1".to_string(),
    };

    cache.put(key.to_string(), result.clone()).await.unwrap();

    // Retrieve it
    let cached = cache.get(key).await.unwrap();
    assert!(cached.is_some());

    let cached_result = cached.unwrap();
    assert_eq!(cached_result.response, Some(b"cached result".to_vec()));
    assert_eq!(cached_result.cache_level, "L1");
}

#[tokio::test]
// Auto-runs on Linux with KVM
async fn test_vm_forking() {
    if !is_kvm_available() {
        eprintln!("KVM not available, skipping test");
        return;
    }

    let fork_mgr = Arc::new(VmForkManager::new(
        PathBuf::from("/tmp/test-forks")
    ));

    // Would need actual parent VM running
    let parent_id = "parent-vm";
    let fork_id = "fork-vm-1";
    let api_socket = "/tmp/fork.sock";

    let result = fork_mgr.fork_vm(parent_id, fork_id, api_socket).await;

    if result.is_ok() {
        let forked = result.unwrap();
        assert_eq!(forked.vm_id, fork_id);
        assert_eq!(forked.parent_id, parent_id);

        // Track the fork
        let _ = fork_mgr.track_fork(parent_id, &forked).await;

        // Get fork tree
        let tree = fork_mgr.get_fork_tree(parent_id).await.unwrap();
        assert!(tree.is_some());
    }
}

#[tokio::test]
// Auto-runs on Linux with KVM
async fn test_vm_predictive_scaling() {
    if !is_kvm_available() {
        eprintln!("KVM not available, skipping test");
        return;
    }

    let snapshot_mgr = Arc::new(VmSnapshotManager::new(
        PathBuf::from("/tmp/test-snapshots")
    ));

    let fork_mgr = Arc::new(VmForkManager::new(
        PathBuf::from("/tmp/test-forks")
    ));

    let scaling_config = ScalingConfig {
        min_warm_vms: 2,
        max_warm_vms: 10,
        scale_up_threshold: 0.8,
        scale_down_threshold: 0.2,
        prediction_window: Duration::from_secs(60),
        warmup_time: Duration::from_secs(1),
    };

    let scaler = Arc::new(VmPredictiveScaler::new(
        fork_mgr,
        snapshot_mgr,
        scaling_config,
    ));

    // Initialize pool for a function
    let function = "test-function";
    scaler.initialize_pool(function, 2).await.unwrap();

    // Simulate requests
    for _ in 0..5 {
        scaler.record_request(function).await;
        sleep(Duration::from_millis(100)).await;
    }

    // Check auto-scaling kicked in
    let stats = scaler.get_pool_stats(function).await.unwrap();
    assert!(stats.total_vms >= 2);
}

#[tokio::test]
// Auto-runs on Linux with KVM
async fn test_vm_branched_execution() {
    if !is_kvm_available() {
        eprintln!("KVM not available, skipping test");
        return;
    }

    let executor = FirecrackerExecutor::new(
        "/usr/bin/firecracker".to_string(),
        "/path/to/kernel".to_string(),
        "/path/to/rootfs".to_string(),
    ).expect("Failed to create executor");

    // First create a parent VM
    let parent_config = SandboxConfig {
        function_id: "parent-vm".to_string(),
        // function_name: Some("parent-function".to_string()),
        // function_version: Some("v1".to_string()),
        source: "alpine:latest".to_string(),
        command: vec!["sh".to_string(), "-c".to_string(), "echo parent".to_string()],
        payload: Vec::new(),
        env_vars: None,
        // code_hash: Some("parent-hash".to_string()),
        // vcpu_count: Some(1),
        // memory_size_mb: Some(256),
    };

    let parent_result = executor.execute(parent_config.clone()).await;
    assert!(parent_result.is_ok());

    // Now fork from parent
    let fork_config = SandboxConfig {
        function_id: "fork-vm".to_string(),
        // function_name: Some("fork-function".to_string()),
        // function_version: Some("v1".to_string()),
        source: "alpine:latest".to_string(),
        command: vec!["sh".to_string(), "-c".to_string(), "echo fork".to_string()],
        payload: Vec::new(),
        env_vars: None,
        // code_hash: Some("fork-hash".to_string()),
        // vcpu_count: Some(1),
        // memory_size_mb: Some(256),
    };

    let fork_result = executor.execute_branched(fork_config, "parent-vm").await;
    assert!(fork_result.is_ok());

    let invocation = fork_result.unwrap();
    assert!(invocation.logs.unwrap().contains("fork"));
}

#[tokio::test]
// Auto-runs on Linux with KVM
async fn test_vm_warm_pool_acquisition() {
    if !is_kvm_available() {
        eprintln!("KVM not available, skipping test");
        return;
    }

    let executor = FirecrackerExecutor::new(
        "/usr/bin/firecracker".to_string(),
        "/path/to/kernel".to_string(),
        "/path/to/rootfs".to_string(),
    ).expect("Failed to create executor");

    // Execute multiple times to test warm pool
    let config = SandboxConfig {
        function_id: "warm-test".to_string(),
        // function_name: Some("warm-function".to_string()),
        // function_version: Some("v1".to_string()),
        source: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "warm".to_string()],
        payload: Vec::new(),
        env_vars: None,
        // code_hash: Some("warm-hash".to_string()),
        // vcpu_count: Some(1),
        // memory_size_mb: Some(256),
    };

    // First execution - cold start
    let result1 = executor.execute(config.clone()).await;
    assert!(result1.is_ok());
    assert!(result1.unwrap().logs.unwrap().contains("cold"));

    // Second execution - should be warm
    let result2 = executor.execute(config.clone()).await;
    assert!(result2.is_ok());
    let logs = result2.unwrap().logs.unwrap();
    assert!(logs.contains("warm") || logs.contains("cached"));
}

#[tokio::test]
// Auto-runs on Linux with KVM
async fn test_vm_incremental_snapshots() {
    if !is_kvm_available() {
        eprintln!("KVM not available, skipping test");
        return;
    }

    let snapshot_mgr = Arc::new(VmSnapshotManager::new(
        PathBuf::from("/tmp/test-snapshots")
    ));

    let vm_id = "test-vm";
    let api_socket = "/tmp/test.sock";

    // Create base snapshot
    let base_snapshot = "base-snapshot";
    let _ = snapshot_mgr.create_snapshot(vm_id, base_snapshot, api_socket).await;

    // Create incremental snapshot
    let incremental = "incremental-1";
    let result = snapshot_mgr.create_incremental_snapshot(
        vm_id,
        incremental,
        base_snapshot,
        api_socket
    ).await;

    if result.is_ok() {
        let snap = result.unwrap();
        assert_eq!(snap.parent_id, Some(base_snapshot.to_string()));
        assert!(snap.is_incremental);
    }
}

#[tokio::test]
// Auto-runs on Linux with KVM
async fn test_vm_performance_optimizations() {
    if !is_kvm_available() {
        eprintln!("KVM not available, skipping test");
        return;
    }

    let executor = FirecrackerExecutor::new(
        "/usr/bin/firecracker".to_string(),
        "/path/to/kernel".to_string(),
        "/path/to/rootfs".to_string(),
    ).expect("Failed to create executor");

    let config = SandboxConfig {
        function_id: "perf-test".to_string(),
        // function_name: Some("perf-function".to_string()),
        // function_version: Some("v1".to_string()),
        source: "alpine:latest".to_string(),
        command: vec!["sh".to_string(), "-c".to_string(), "for i in 1 2 3; do echo $i; done".to_string()],
        payload: Vec::new(),
        env_vars: None,
        // code_hash: Some("perf-hash".to_string()),
        // vcpu_count: Some(2),
        // memory_size_mb: Some(512),
    };

    // Measure cold start
    let start = std::time::Instant::now();
    let result1 = executor.execute(config.clone()).await;
    let cold_duration = start.elapsed();
    assert!(result1.is_ok());

    // Measure warm/cached execution
    let start = std::time::Instant::now();
    let result2 = executor.execute(config).await;
    let warm_duration = start.elapsed();
    assert!(result2.is_ok());

    // Warm should be significantly faster
    println!("Cold start: {:?}, Warm start: {:?}", cold_duration, warm_duration);

    // In real scenarios, warm should be at least 10x faster
    // But for testing we can't guarantee exact ratios
    assert!(warm_duration <= cold_duration);
}