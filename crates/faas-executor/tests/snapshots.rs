use faas_executor::snapshot::{
    FilesystemState, Snapshot, SnapshotHasher, SnapshotManifest, SnapshotMetadata, SnapshotStorage,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tempfile::tempdir;
use tokio::fs;

/// Test deterministic snapshot hashing core functionality
#[tokio::test]
async fn test_hash_determinism() {
    let mut hasher1 = SnapshotHasher::new();
    let mut hasher2 = SnapshotHasher::new();

    let memory_data = b"test memory content with some data";
    let fs_state = create_test_filesystem();

    let hash1 = hasher1.hash_snapshot(memory_data, &fs_state, "alpine:latest", None);
    let hash2 = hasher2.hash_snapshot(memory_data, &fs_state, "alpine:latest", None);

    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 64); // SHA256 hex string
    assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
}

#[tokio::test]
async fn test_hash_sensitivity() {
    let mut hasher = SnapshotHasher::new();
    let memory_data = b"test data";
    let fs_state = create_test_filesystem();

    let hash1 = hasher.hash_snapshot(memory_data, &fs_state, "alpine:latest", None);

    // Different memory content
    let hash2 = hasher.hash_snapshot(b"different data", &fs_state, "alpine:latest", None);
    assert_ne!(hash1, hash2);

    // Different environment
    let hash3 = hasher.hash_snapshot(memory_data, &fs_state, "ubuntu:latest", None);
    assert_ne!(hash1, hash3);
}

#[tokio::test]
async fn test_parent_hash_chaining() {
    let mut hasher = SnapshotHasher::new();
    let memory_data = b"test data";
    let fs_state = create_test_filesystem();

    let parent_hash = hasher.hash_snapshot(memory_data, &fs_state, "alpine:latest", None);
    let child_hash =
        hasher.hash_snapshot(memory_data, &fs_state, "alpine:latest", Some(&parent_hash));

    assert_ne!(parent_hash, child_hash);
    assert!(child_hash.len() == 64);
}

#[tokio::test]
async fn test_filesystem_ordering() {
    let mut hasher = SnapshotHasher::new();
    let memory_data = b"test";

    // Create filesystem state with files in different orders
    let mut fs_state1 = FilesystemState {
        root: PathBuf::from("/test"),
        files: BTreeMap::new(),
    };

    let mut fs_state2 = FilesystemState {
        root: PathBuf::from("/test"),
        files: BTreeMap::new(),
    };

    // Add files in different orders
    fs_state1
        .files
        .insert("a.txt".to_string(), create_file_metadata(b"content1"));
    fs_state1
        .files
        .insert("b.txt".to_string(), create_file_metadata(b"content2"));
    fs_state1
        .files
        .insert("c.txt".to_string(), create_file_metadata(b"content3"));

    fs_state2
        .files
        .insert("c.txt".to_string(), create_file_metadata(b"content3"));
    fs_state2
        .files
        .insert("a.txt".to_string(), create_file_metadata(b"content1"));
    fs_state2
        .files
        .insert("b.txt".to_string(), create_file_metadata(b"content2"));

    let hash1 = hasher.hash_snapshot(memory_data, &fs_state1, "alpine:latest", None);
    let hash2 = hasher.hash_snapshot(memory_data, &fs_state2, "alpine:latest", None);

    assert_eq!(hash1, hash2, "File order should not affect hash");
}

#[tokio::test]
async fn test_large_memory_chunking() {
    let mut hasher = SnapshotHasher::new();
    let fs_state = create_test_filesystem();

    // Test with large memory data (2MB)
    let large_data = vec![0x42u8; 2 * 1024 * 1024];
    let hash = hasher.hash_snapshot(&large_data, &fs_state, "alpine:latest", None);

    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

    // Same data should produce same hash
    let hash2 = hasher.hash_snapshot(&large_data, &fs_state, "alpine:latest", None);
    assert_eq!(hash, hash2);
}

#[tokio::test]
async fn test_snapshot_storage_content_addressing() {
    let temp_dir = tempdir().unwrap();
    let storage = SnapshotStorage::new(temp_dir.path());

    let snapshot = create_test_snapshot("test_hash_123");
    let memory_data = b"memory content";
    let fs_data = b"filesystem content";

    // Store snapshot
    storage
        .store_snapshot(&snapshot, memory_data, fs_data)
        .await
        .unwrap();

    // Verify content-addressed storage
    assert!(storage.exists(&snapshot.id).await);
    assert!(storage.exists(&snapshot.content_hash).await);

    // Load by ID
    let (loaded_by_id, mem1, fs1) = storage.load_snapshot(&snapshot.id).await.unwrap();
    assert_eq!(loaded_by_id.content_hash, snapshot.content_hash);
    assert_eq!(mem1, memory_data);
    assert_eq!(fs1, fs_data);

    // Load by content hash
    let (loaded_by_hash, mem2, fs2) = storage.load_snapshot(&snapshot.content_hash).await.unwrap();
    assert_eq!(loaded_by_hash.id, snapshot.id);
    assert_eq!(mem2, memory_data);
    assert_eq!(fs2, fs_data);
}

#[tokio::test]
async fn test_snapshot_deduplication() {
    let temp_dir = tempdir().unwrap();
    let storage = SnapshotStorage::new(temp_dir.path());

    let memory_data = b"shared memory";
    let fs_data = b"shared filesystem";

    // Create two snapshots with same content hash
    let snapshot1 = Snapshot {
        id: "snap1".to_string(),
        content_hash: "shared_hash".to_string(),
        parent_hash: None,
        metadata: create_test_metadata(),
        manifest: create_test_manifest(),
    };

    let snapshot2 = Snapshot {
        id: "snap2".to_string(),
        content_hash: "shared_hash".to_string(),
        parent_hash: None,
        metadata: create_test_metadata(),
        manifest: create_test_manifest(),
    };

    // Store both
    storage
        .store_snapshot(&snapshot1, memory_data, fs_data)
        .await
        .unwrap();
    storage
        .store_snapshot(&snapshot2, memory_data, fs_data)
        .await
        .unwrap();

    // Both should be accessible
    assert!(storage.exists(&snapshot1.id).await);
    assert!(storage.exists(&snapshot2.id).await);
    assert!(storage.exists("shared_hash").await);

    // Content should be identical
    let (loaded1, _, _) = storage.load_snapshot(&snapshot1.id).await.unwrap();
    let (loaded2, _, _) = storage.load_snapshot(&snapshot2.id).await.unwrap();
    assert_eq!(loaded1.content_hash, loaded2.content_hash);
}

#[tokio::test]
async fn test_filesystem_scanning() {
    let temp_dir = tempdir().unwrap();
    let scan_path = temp_dir.path();

    // Create directory structure
    fs::write(scan_path.join("file1.txt"), b"content1")
        .await
        .unwrap();
    fs::write(scan_path.join("file2.log"), b"log content")
        .await
        .unwrap();

    let subdir = scan_path.join("subdir");
    fs::create_dir(&subdir).await.unwrap();
    fs::write(subdir.join("nested.txt"), b"nested content")
        .await
        .unwrap();

    let deep_dir = subdir.join("deep");
    fs::create_dir(&deep_dir).await.unwrap();
    fs::write(deep_dir.join("deep.txt"), b"deep content")
        .await
        .unwrap();

    // Scan filesystem
    let fs_state = FilesystemState::from_directory(scan_path).await.unwrap();

    assert_eq!(fs_state.files.len(), 4);
    assert!(fs_state.files.contains_key("file1.txt"));
    assert!(fs_state.files.contains_key("file2.log"));
    assert!(fs_state.files.contains_key("subdir/nested.txt"));
    assert!(fs_state.files.contains_key("subdir/deep/deep.txt"));

    // Verify file metadata
    let file1_meta = &fs_state.files["file1.txt"];
    assert_eq!(file1_meta.size, 8); // "content1".len()
    assert!(!file1_meta.hash.is_empty());
}

#[tokio::test]
async fn test_empty_filesystem() {
    let temp_dir = tempdir().unwrap();
    let fs_state = FilesystemState::from_directory(temp_dir.path())
        .await
        .unwrap();

    assert_eq!(fs_state.files.len(), 0);

    let mut hasher = SnapshotHasher::new();
    let hash = hasher.hash_snapshot(b"test", &fs_state, "alpine:latest", None);
    assert_eq!(hash.len(), 64);
}

#[tokio::test]
async fn test_symlink_handling() {
    let temp_dir = tempdir().unwrap();
    let scan_path = temp_dir.path();

    // Create file and symlink
    let original = scan_path.join("original.txt");
    fs::write(&original, b"original content").await.unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link_path = scan_path.join("link.txt");
        symlink(&original, &link_path).unwrap();

        // Scanning should handle symlinks gracefully
        let fs_state = FilesystemState::from_directory(scan_path).await.unwrap();

        // Should contain original file, symlink handling depends on implementation
        assert!(fs_state.files.contains_key("original.txt"));
    }
}

// Helper functions

fn create_test_filesystem() -> FilesystemState {
    let mut fs_state = FilesystemState {
        root: PathBuf::from("/test"),
        files: BTreeMap::new(),
    };

    fs_state
        .files
        .insert("file1.txt".to_string(), create_file_metadata(b"content1"));
    fs_state
        .files
        .insert("file2.txt".to_string(), create_file_metadata(b"content2"));
    fs_state
}

fn create_file_metadata(content: &[u8]) -> faas_executor::snapshot::FileMetadata {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(content).to_vec();

    faas_executor::snapshot::FileMetadata {
        hash,
        size: content.len() as u64,
        mode: 0o644,
        modified: 1234567890,
    }
}

fn create_test_snapshot(content_hash: &str) -> Snapshot {
    Snapshot {
        id: format!("snap_{}", uuid::Uuid::new_v4()),
        content_hash: content_hash.to_string(),
        parent_hash: None,
        metadata: create_test_metadata(),
        manifest: create_test_manifest(),
    }
}

fn create_test_metadata() -> SnapshotMetadata {
    SnapshotMetadata {
        created_at: chrono::Utc::now(),
        mode: faas_common::ExecutionMode::Checkpointed,
        environment: "alpine:latest".to_string(),
        tags: vec!["test".to_string()],
        labels: BTreeMap::new(),
    }
}

fn create_test_manifest() -> SnapshotManifest {
    SnapshotManifest {
        memory_hash: "memory_hash".to_string(),
        filesystem_hash: "fs_hash".to_string(),
        environment_hash: "env_hash".to_string(),
        total_size: 1024,
        memory_pages: 256,
        files: vec![],
    }
}
