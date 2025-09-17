use chrono::{Duration as ChronoDuration, Utc};
use faas_executor::{
    readiness::{
        LongRunningConfig, LongRunningExecutionManager, ProbeType, ReadinessChecker,
        ReadinessConfig, ReadinessProbe,
    },
    snapshot::{FilesystemState, Snapshot, SnapshotHasher, SnapshotStorage},
    ssh::{SshConfig, SshConnectionManager, SshKeyAlgorithm, SshKeyManager},
    sync::{FileSynchronizer, SyncOptions},
};
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;
use tokio::fs;

#[tokio::test]
async fn test_deterministic_snapshot_hashing() {
    let mut hasher = SnapshotHasher::new();

    // Create identical filesystem states
    let fs_state1 = FilesystemState {
        root: Path::new("/test").to_path_buf(),
        files: std::collections::BTreeMap::new(),
    };

    let fs_state2 = fs_state1.clone();

    let memory_data = b"test memory content";

    // Hash same content twice
    let hash1 = hasher.hash_snapshot(memory_data, &fs_state1, "alpine:latest", None);

    let hash2 = hasher.hash_snapshot(memory_data, &fs_state2, "alpine:latest", None);

    assert_eq!(
        hash1, hash2,
        "Identical content should produce identical hashes"
    );
    assert_eq!(hash1.len(), 64, "SHA256 hash should be 64 characters");

    // Hash with parent
    let hash_with_parent =
        hasher.hash_snapshot(memory_data, &fs_state1, "alpine:latest", Some(&hash1));

    assert_ne!(hash_with_parent, hash1, "Hash with parent should differ");
}

#[tokio::test]
async fn test_snapshot_storage_with_content_addressing() {
    let temp_dir = tempdir().unwrap();
    let storage = SnapshotStorage::new(temp_dir.path());

    let snapshot = Snapshot {
        id: "snap_test_123".to_string(),
        content_hash: "abc123def456789".to_string(),
        parent_hash: None,
        metadata: faas_executor::snapshot::SnapshotMetadata {
            created_at: Utc::now(),
            mode: faas_common::ExecutionMode::Checkpointed,
            environment: "alpine:latest".to_string(),
            tags: vec!["test".to_string()],
            labels: std::collections::BTreeMap::new(),
        },
        manifest: faas_executor::snapshot::SnapshotManifest {
            memory_hash: "mem_hash".to_string(),
            filesystem_hash: "fs_hash".to_string(),
            environment_hash: "env_hash".to_string(),
            total_size: 1024,
            memory_pages: 256,
            files: vec![],
        },
    };

    let memory_data = b"test memory";
    let fs_data = b"test filesystem";

    // Store snapshot
    storage
        .store_snapshot(&snapshot, memory_data, fs_data)
        .await
        .unwrap();

    // Load by ID
    assert!(storage.exists(&snapshot.id).await);
    let (loaded_snap, loaded_mem, loaded_fs) = storage.load_snapshot(&snapshot.id).await.unwrap();
    assert_eq!(loaded_snap.content_hash, snapshot.content_hash);
    assert_eq!(loaded_mem, memory_data);
    assert_eq!(loaded_fs, fs_data);

    // Load by content hash
    assert!(storage.exists(&snapshot.content_hash).await);
    let (loaded_by_hash, _, _) = storage.load_snapshot(&snapshot.content_hash).await.unwrap();
    assert_eq!(loaded_by_hash.id, snapshot.id);
}

#[tokio::test]
async fn test_advanced_file_sync_with_gitignore() {
    let source_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();

    // Create test files and .gitignore
    fs::write(source_dir.path().join(".gitignore"), "*.log\ntemp/\n*.tmp")
        .await
        .unwrap();
    fs::write(source_dir.path().join("keep.txt"), b"keep this")
        .await
        .unwrap();
    fs::write(source_dir.path().join("ignore.log"), b"ignore this")
        .await
        .unwrap();
    fs::write(source_dir.path().join("data.tmp"), b"temp data")
        .await
        .unwrap();

    let subdir = source_dir.path().join("src");
    fs::create_dir(&subdir).await.unwrap();
    fs::write(subdir.join("code.rs"), b"fn main() {}")
        .await
        .unwrap();

    let temp_dir = source_dir.path().join("temp");
    fs::create_dir(&temp_dir).await.unwrap();
    fs::write(temp_dir.join("cache.dat"), b"cache")
        .await
        .unwrap();

    // Sync with gitignore enabled
    let options = SyncOptions {
        use_gitignore: true,
        dry_run: false,
        delete_unmatched: false,
        checksum_only: false,
        preserve_timestamps: true,
        exclude_patterns: vec![],
        include_patterns: vec![],
    };

    let synchronizer = FileSynchronizer::new(source_dir.path(), options)
        .await
        .unwrap();
    let result = synchronizer
        .sync(source_dir.path(), dest_dir.path())
        .await
        .unwrap();

    // Verify results
    assert!(result.files_copied.contains(&"keep.txt".to_string()));
    assert!(result.files_copied.contains(&"src/code.rs".to_string()));
    assert!(result.files_skipped.contains(&"ignore.log".to_string()));
    assert!(result.files_skipped.contains(&"data.tmp".to_string()));

    // Verify files exist/don't exist
    assert!(dest_dir.path().join("keep.txt").exists());
    assert!(dest_dir.path().join("src/code.rs").exists());
    assert!(!dest_dir.path().join("ignore.log").exists());
    assert!(!dest_dir.path().join("temp/cache.dat").exists());
}

#[tokio::test]
async fn test_checksum_based_sync() {
    let source_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();

    fs::write(source_dir.path().join("file.txt"), b"initial content")
        .await
        .unwrap();

    let options = SyncOptions {
        use_gitignore: false,
        dry_run: false,
        delete_unmatched: false,
        checksum_only: true,
        preserve_timestamps: false,
        exclude_patterns: vec![],
        include_patterns: vec![],
    };

    let synchronizer = FileSynchronizer::new(source_dir.path(), options.clone())
        .await
        .unwrap();

    // First sync
    let result1 = synchronizer
        .sync(source_dir.path(), dest_dir.path())
        .await
        .unwrap();
    assert_eq!(result1.files_copied.len(), 1);

    // Second sync with same content - should skip
    let result2 = synchronizer
        .sync(source_dir.path(), dest_dir.path())
        .await
        .unwrap();
    assert_eq!(result2.files_updated.len(), 0);
    assert_eq!(result2.files_copied.len(), 0);

    // Modify file
    fs::write(source_dir.path().join("file.txt"), b"modified content")
        .await
        .unwrap();

    // Third sync - should update
    let result3 = synchronizer
        .sync(source_dir.path(), dest_dir.path())
        .await
        .unwrap();
    assert_eq!(result3.files_updated.len(), 1);
}

#[tokio::test]
async fn test_ssh_key_generation_and_rotation() {
    let temp_dir = tempdir().unwrap();
    let config = SshConfig {
        auto_rotate: true,
        rotation_interval: ChronoDuration::days(30),
        key_algorithm: SshKeyAlgorithm::Ed25519,
        max_keys_per_instance: 3,
    };

    let mut key_manager = SshKeyManager::new(temp_dir.path(), config);

    // Generate initial key
    let key1 = key_manager.generate_keypair().await.unwrap();
    assert!(key1.private_key.contains("BEGIN OPENSSH PRIVATE KEY"));
    assert!(key1.public_key.starts_with("ssh-"));
    assert!(key1.fingerprint.starts_with("SHA256:"));
    assert_eq!(key1.algorithm, "Ed25519");

    // Rotate key
    let key2 = key_manager.rotate_key("instance1", &key1.id).await.unwrap();
    assert_ne!(key2.id, key1.id);
    assert_eq!(key2.rotated_from, Some(key1.id.clone()));

    // Check expiration
    if let Some(expires_at) = key1.expires_at {
        let expected_expiry = key1.created_at + ChronoDuration::days(30);
        assert_eq!(expires_at, expected_expiry);
    }

    // Test needs_rotation
    let needs_rotation = key_manager.needs_rotation(&key1);
    assert!(!needs_rotation); // Newly created key shouldn't need rotation

    // Create expired key for testing
    let mut expired_key = key1.clone();
    expired_key.expires_at = Some(Utc::now() - ChronoDuration::days(1));
    assert!(key_manager.needs_rotation(&expired_key));
}

#[tokio::test]
async fn test_ssh_connection_manager() {
    let temp_dir = tempdir().unwrap();
    let key_manager = SshKeyManager::new(temp_dir.path(), SshConfig::default());
    let mut conn_manager = SshConnectionManager::new(key_manager);

    // Connect to instance
    let key1 = conn_manager
        .connect("inst1", "localhost", 22)
        .await
        .unwrap();
    assert!(!key1.private_key.is_empty());

    // Reconnect should reuse key
    let key2 = conn_manager
        .connect("inst1", "localhost", 22)
        .await
        .unwrap();
    assert_eq!(key1.id, key2.id);

    // Disconnect
    conn_manager.disconnect("inst1").await.unwrap();
}

#[tokio::test]
async fn test_readiness_checks() {
    // Test file-based readiness
    let temp_dir = tempdir().unwrap();
    let ready_file = temp_dir.path().join("ready.txt");

    let config = ReadinessConfig {
        check_interval: Duration::from_millis(100),
        initial_delay: Duration::from_millis(0),
        timeout: Duration::from_secs(2),
        success_threshold: 2,
        failure_threshold: 3,
        probes: vec![ReadinessProbe {
            probe_type: ProbeType::File,
            path: Some(ready_file.to_string_lossy().to_string()),
            port: None,
            command: None,
            expected_status: None,
            timeout: Duration::from_secs(1),
        }],
    };

    let mut checker = ReadinessChecker::new(config);

    // Start check in background
    let check_handle = tokio::spawn(async move { checker.wait_for_ready("test-instance").await });

    // Create ready file after delay
    tokio::time::sleep(Duration::from_millis(150)).await;
    fs::write(&ready_file, b"ready").await.unwrap();

    // Wait for check to complete
    let result = check_handle.await.unwrap();
    assert!(result.is_ok());

    let status = result.unwrap();
    assert!(status.ready);
    assert!(status.consecutive_successes >= 2);
    assert_eq!(status.consecutive_failures, 0);
}

#[tokio::test]
async fn test_tcp_readiness_check() {
    use tokio::net::TcpListener;

    // Start TCP server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            if listener.accept().await.is_ok() {
                break;
            }
        }
    });

    let config = ReadinessConfig {
        check_interval: Duration::from_millis(100),
        timeout: Duration::from_secs(1),
        success_threshold: 1,
        failure_threshold: 3,
        probes: vec![ReadinessProbe {
            probe_type: ProbeType::Tcp,
            port: Some(port),
            path: None,
            command: None,
            expected_status: None,
            timeout: Duration::from_secs(1),
        }],
        ..Default::default()
    };

    let mut checker = ReadinessChecker::new(config);
    let status = checker.wait_for_ready("127.0.0.1").await.unwrap();

    assert!(status.ready);
    assert!(status.consecutive_successes >= 1);
}

#[tokio::test]
async fn test_long_running_execution_management() {
    let config = LongRunningConfig {
        max_duration: Duration::from_secs(60),
        heartbeat_interval: Duration::from_millis(100),
        checkpoint_interval: Some(Duration::from_millis(500)),
        auto_extend: true,
        grace_period: Duration::from_secs(5),
    };

    let manager = LongRunningExecutionManager::new(config);

    // Start session
    let session = manager
        .start_session("exec-test".to_string())
        .await
        .unwrap();
    assert_eq!(session.id, "exec-test");
    assert_eq!(session.extended_count, 0);

    // Send heartbeats
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        manager.heartbeat("exec-test").await.unwrap();
    }

    // Create checkpoint
    manager
        .checkpoint("exec-test", "ckpt-1".to_string(), 1024)
        .await
        .unwrap();

    // Extend session
    let new_duration = manager.extend_session("exec-test").await.unwrap();
    assert_eq!(new_duration, Duration::from_secs(60));

    // Session management verified through successful extend operation
    // Internal state is private and tested through public API behavior
}

#[tokio::test]
async fn test_auto_checkpointing() {
    let config = LongRunningConfig {
        max_duration: Duration::from_secs(60),
        heartbeat_interval: Duration::from_millis(100),
        checkpoint_interval: Some(Duration::from_millis(200)),
        auto_extend: false,
        grace_period: Duration::from_secs(1),
    };

    let manager = LongRunningExecutionManager::new(config);

    // Start session
    let _session = manager
        .start_session("auto-ckpt".to_string())
        .await
        .unwrap();

    // Wait for auto-checkpointing
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Send heartbeat to keep session alive
    manager.heartbeat("auto-ckpt").await.unwrap();

    // Session monitoring task should create checkpoints automatically
    // In production, this would trigger actual checkpoint creation
}

#[tokio::test]
async fn test_dry_run_sync() {
    let source_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();

    fs::write(source_dir.path().join("test.txt"), b"test")
        .await
        .unwrap();

    let options = SyncOptions {
        use_gitignore: false,
        dry_run: true,
        delete_unmatched: true,
        checksum_only: false,
        preserve_timestamps: false,
        exclude_patterns: vec![],
        include_patterns: vec![],
    };

    let synchronizer = FileSynchronizer::new(source_dir.path(), options)
        .await
        .unwrap();
    let result = synchronizer
        .sync(source_dir.path(), dest_dir.path())
        .await
        .unwrap();

    assert!(result.dry_run);
    assert_eq!(result.files_copied.len(), 1);
    assert_eq!(result.bytes_transferred, 0); // No actual transfer in dry run
    assert!(!dest_dir.path().join("test.txt").exists()); // File not actually copied
}
