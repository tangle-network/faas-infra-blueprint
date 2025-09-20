
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Deterministic snapshot with content-based addressing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub content_hash: String,
    pub parent_hash: Option<String>,
    pub metadata: SnapshotMetadata,
    pub manifest: SnapshotManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub created_at: DateTime<Utc>,
    pub mode: faas_common::ExecutionMode,
    pub environment: String,
    pub tags: Vec<String>,
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub memory_hash: String,
    pub filesystem_hash: String,
    pub environment_hash: String,
    pub total_size: u64,
    pub memory_pages: u64,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub mode: u32,
    pub modified: i64,
}

/// Generates deterministic content-based hash for snapshots
pub struct SnapshotHasher {
    hasher: Sha256,
}

impl SnapshotHasher {
    pub fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    /// Generate deterministic hash from snapshot content
    pub fn hash_snapshot(
        &mut self,
        memory_data: &[u8],
        filesystem_state: &FilesystemState,
        environment: &str,
        parent_hash: Option<&str>,
    ) -> String {
        // Reset hasher
        self.hasher = Sha256::new();

        // Include parent hash for chain integrity
        if let Some(parent) = parent_hash {
            self.hasher.update(parent.as_bytes());
        }

        // Hash memory content
        let memory_hash = self.hash_memory(memory_data);
        self.hasher.update(&memory_hash);

        // Hash filesystem state
        let fs_hash = self.hash_filesystem(filesystem_state);
        self.hasher.update(&fs_hash);

        // Hash environment
        let env_hash = self.hash_environment(environment);
        self.hasher.update(&env_hash);

        // Generate final hash
        format!("{:x}", self.hasher.finalize_reset())
    }

    fn hash_memory(&mut self, data: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();

        // Process in chunks for large memory snapshots
        const CHUNK_SIZE: usize = 1024 * 1024; // 1MB chunks
        for chunk in data.chunks(CHUNK_SIZE) {
            hasher.update(chunk);
        }

        hasher.finalize().to_vec()
    }

    fn hash_filesystem(&mut self, state: &FilesystemState) -> Vec<u8> {
        let mut hasher = Sha256::new();

        // Sort files for deterministic ordering
        let mut files: Vec<_> = state.files.iter().collect();
        files.sort_by(|a, b| a.0.cmp(b.0));

        for (path, entry) in files {
            hasher.update(path.as_bytes());
            hasher.update(&entry.hash);
            hasher.update(&entry.size.to_le_bytes());
            hasher.update(&entry.mode.to_le_bytes());
        }

        hasher.finalize().to_vec()
    }

    fn hash_environment(&mut self, env: &str) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(env.as_bytes());
        hasher.finalize().to_vec()
    }
}

/// Filesystem state for deterministic hashing
#[derive(Debug, Clone)]
pub struct FilesystemState {
    pub root: PathBuf,
    pub files: BTreeMap<String, FileMetadata>,
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub hash: Vec<u8>,
    pub size: u64,
    pub mode: u32,
    pub modified: i64,
}

impl FilesystemState {
    pub async fn from_directory(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let mut files = BTreeMap::new();
        Self::scan_directory(path, path, &mut files).await?;

        Ok(Self {
            root: path.to_path_buf(),
            files,
        })
    }

    async fn scan_directory(
        root: &Path,
        current: &Path,
        files: &mut BTreeMap<String, FileMetadata>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut entries = fs::read_dir(current).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let metadata = entry.metadata().await?;

            if metadata.is_file() {
                let relative_path = path.strip_prefix(root)?.to_string_lossy().to_string();

                let content = fs::read(&path).await?;
                let hash = Sha256::digest(&content).to_vec();

                files.insert(
                    relative_path,
                    FileMetadata {
                        hash,
                        size: metadata.len(),
                        mode: 0o644, // Simplified for now
                        modified: metadata
                            .modified()?
                            .duration_since(std::time::UNIX_EPOCH)?
                            .as_secs() as i64,
                    },
                );
            } else if metadata.is_dir() {
                Box::pin(Self::scan_directory(root, &path, files)).await?;
            }
        }

        Ok(())
    }
}

/// Snapshot storage with content-addressed storage
pub struct SnapshotStorage {
    base_path: PathBuf,
}

impl SnapshotStorage {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    pub async fn store_snapshot(
        &self,
        snapshot: &Snapshot,
        memory_data: &[u8],
        filesystem_data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let snapshot_dir = self.base_path.join(&snapshot.content_hash);
        fs::create_dir_all(&snapshot_dir).await?;

        // Store metadata
        let metadata_path = snapshot_dir.join("metadata.json");
        let metadata_json = serde_json::to_vec_pretty(snapshot)?;
        fs::write(metadata_path, metadata_json).await?;

        // Store memory snapshot
        let memory_path = snapshot_dir.join("memory.bin");
        fs::write(memory_path, memory_data).await?;

        // Store filesystem snapshot
        let fs_path = snapshot_dir.join("filesystem.tar");
        fs::write(fs_path, filesystem_data).await?;

        // Create symlink for easy access by ID
        let id_link = self.base_path.join(&snapshot.id);
        if id_link.exists() {
            fs::remove_file(&id_link).await.ok();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&snapshot_dir, &id_link)?;
        }

        Ok(())
    }

    pub async fn load_snapshot(
        &self,
        hash_or_id: &str,
    ) -> Result<(Snapshot, Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
        let snapshot_dir = if self
            .base_path
            .join(hash_or_id)
            .join("metadata.json")
            .exists()
        {
            self.base_path.join(hash_or_id)
        } else {
            // Try following symlink
            fs::read_link(self.base_path.join(hash_or_id)).await?
        };

        // Load metadata
        let metadata_json = fs::read(snapshot_dir.join("metadata.json")).await?;
        let snapshot: Snapshot = serde_json::from_slice(&metadata_json)?;

        // Load memory data
        let memory_data = fs::read(snapshot_dir.join("memory.bin")).await?;

        // Load filesystem data
        let filesystem_data = fs::read(snapshot_dir.join("filesystem.tar")).await?;

        Ok((snapshot, memory_data, filesystem_data))
    }

    pub async fn exists(&self, hash_or_id: &str) -> bool {
        self.base_path.join(hash_or_id).exists()
            || self
                .base_path
                .join(hash_or_id)
                .join("metadata.json")
                .exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_deterministic_hashing() {
        let mut hasher1 = SnapshotHasher::new();
        let mut hasher2 = SnapshotHasher::new();

        let memory_data = b"test memory data";
        let mut fs_state = FilesystemState {
            root: PathBuf::from("/test"),
            files: BTreeMap::new(),
        };

        fs_state.files.insert(
            "file1.txt".to_string(),
            FileMetadata {
                hash: vec![1, 2, 3],
                size: 100,
                mode: 0o644,
                modified: 1234567890,
            },
        );

        let hash1 = hasher1.hash_snapshot(memory_data, &fs_state, "alpine:latest", None);

        let hash2 = hasher2.hash_snapshot(memory_data, &fs_state, "alpine:latest", None);

        assert_eq!(hash1, hash2, "Hashes should be deterministic");
    }

    #[tokio::test]
    async fn test_snapshot_storage() {
        let temp_dir = tempdir().unwrap();
        let storage = SnapshotStorage::new(temp_dir.path());

        let snapshot = Snapshot {
            id: "snap_123".to_string(),
            content_hash: "abc123def456".to_string(),
            parent_hash: None,
            metadata: SnapshotMetadata {
                created_at: Utc::now(),
                mode: faas_common::ExecutionMode::Checkpointed,
                environment: "alpine:latest".to_string(),
                tags: vec!["test".to_string()],
                labels: BTreeMap::new(),
            },
            manifest: SnapshotManifest {
                memory_hash: "mem123".to_string(),
                filesystem_hash: "fs456".to_string(),
                environment_hash: "env789".to_string(),
                total_size: 1024,
                memory_pages: 256,
                files: vec![],
            },
        };

        let memory_data = b"test memory";
        let fs_data = b"test filesystem";

        storage
            .store_snapshot(&snapshot, memory_data, fs_data)
            .await
            .unwrap();

        assert!(storage.exists(&snapshot.id).await);
        assert!(storage.exists(&snapshot.content_hash).await);

        let (loaded, mem, fs) = storage.load_snapshot(&snapshot.id).await.unwrap();
        assert_eq!(loaded.content_hash, snapshot.content_hash);
        assert_eq!(mem, memory_data);
        assert_eq!(fs, fs_data);
    }

    #[tokio::test]
    async fn test_filesystem_state_scanning() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, b"test content").await.unwrap();

        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).await.unwrap();
        let nested_file = subdir.join("nested.txt");
        fs::write(&nested_file, b"nested content").await.unwrap();

        let fs_state = FilesystemState::from_directory(temp_dir.path())
            .await
            .unwrap();

        assert_eq!(fs_state.files.len(), 2);
        assert!(fs_state.files.contains_key("test.txt"));
        assert!(fs_state.files.contains_key("subdir/nested.txt"));
    }
}
