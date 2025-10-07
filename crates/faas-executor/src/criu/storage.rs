//! Storage backends for CRIU checkpoints

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info};

use super::CheckpointMetadata;

/// Abstract checkpoint storage backend
#[async_trait]
pub trait CheckpointStorage: Send + Sync {
    /// Store a checkpoint
    async fn store(&self, checkpoint_id: &str, checkpoint_path: &Path) -> Result<()>;

    /// Retrieve a checkpoint
    async fn retrieve(&self, checkpoint_id: &str) -> Result<PathBuf>;

    /// Delete a checkpoint
    async fn delete(&self, checkpoint_id: &str) -> Result<()>;

    /// List all checkpoints
    async fn list(&self) -> Result<Vec<CheckpointMetadata>>;

    /// Check if checkpoint exists
    async fn exists(&self, checkpoint_id: &str) -> Result<bool>;
}

/// Local filesystem storage for checkpoints
pub struct LocalCheckpointStorage {
    base_dir: PathBuf,
}

impl LocalCheckpointStorage {
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }
}

#[async_trait]
impl CheckpointStorage for LocalCheckpointStorage {
    async fn store(&self, checkpoint_id: &str, checkpoint_path: &Path) -> Result<()> {
        let dest_dir = self.base_dir.join(checkpoint_id);
        fs::create_dir_all(&dest_dir).await?;

        if checkpoint_path.is_file() {
            // Single file (compressed checkpoint)
            let filename = checkpoint_path
                .file_name()
                .ok_or_else(|| anyhow!("Invalid checkpoint path"))?;
            let dest_file = dest_dir.join(filename);
            fs::copy(checkpoint_path, &dest_file).await?;
            info!("Stored checkpoint {} to {}", checkpoint_id, dest_file.display());
        } else if checkpoint_path.is_dir() {
            // Directory (uncompressed checkpoint)
            copy_dir_recursive(checkpoint_path, &dest_dir).await?;
            info!("Stored checkpoint {} to {}", checkpoint_id, dest_dir.display());
        } else {
            return Err(anyhow!("Checkpoint path is neither file nor directory"));
        }

        Ok(())
    }

    async fn retrieve(&self, checkpoint_id: &str) -> Result<PathBuf> {
        let checkpoint_dir = self.base_dir.join(checkpoint_id);

        if !checkpoint_dir.exists() {
            return Err(anyhow!("Checkpoint {} not found", checkpoint_id));
        }

        debug!("Retrieved checkpoint {} from {}", checkpoint_id, checkpoint_dir.display());
        Ok(checkpoint_dir)
    }

    async fn delete(&self, checkpoint_id: &str) -> Result<()> {
        let checkpoint_dir = self.base_dir.join(checkpoint_id);

        if checkpoint_dir.exists() {
            fs::remove_dir_all(&checkpoint_dir).await?;
            info!("Deleted checkpoint {}", checkpoint_id);
        }

        Ok(())
    }

    async fn list(&self) -> Result<Vec<CheckpointMetadata>> {
        let mut checkpoints = Vec::new();
        let mut entries = fs::read_dir(&self.base_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let checkpoint_id = entry.file_name().to_string_lossy().to_string();

                // Try to load metadata
                let metadata_path = entry.path().join("metadata.json");
                if metadata_path.exists() {
                    let metadata_str = fs::read_to_string(&metadata_path).await?;
                    if let Ok(metadata) = serde_json::from_str::<CheckpointMetadata>(&metadata_str) {
                        checkpoints.push(metadata);
                        continue;
                    }
                }

                // Fallback: create basic metadata
                let size = calculate_dir_size(&entry.path()).await?;
                checkpoints.push(CheckpointMetadata {
                    id: checkpoint_id.clone(),
                    container_id: "unknown".to_string(),
                    created_at: chrono::Utc::now(),
                    size_bytes: size,
                    compressed: false,
                    compression_format: None,
                });
            }
        }

        Ok(checkpoints)
    }

    async fn exists(&self, checkpoint_id: &str) -> Result<bool> {
        let checkpoint_dir = self.base_dir.join(checkpoint_id);
        Ok(checkpoint_dir.exists())
    }
}

/// Recursively copy a directory
async fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).await?;

    let mut entries = fs::read_dir(src).await?;

    while let Some(entry) = entries.next_entry().await? {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if entry.file_type().await?.is_dir() {
            copy_dir_recursive(&src_path, &dst_path).await?;
        } else {
            fs::copy(&src_path, &dst_path).await?;
        }
    }

    Ok(())
}

/// Calculate total size of a directory
async fn calculate_dir_size(dir: &Path) -> Result<u64> {
    let mut size = 0u64;
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let metadata = entry.metadata().await?;
        if metadata.is_file() {
            size += metadata.len();
        } else if metadata.is_dir() {
            size += calculate_dir_size(&entry.path()).await?;
        }
    }

    Ok(size)
}

/// S3-compatible storage backend (future implementation)
pub struct S3CheckpointStorage {
    _bucket: String,
    _region: String,
}

impl S3CheckpointStorage {
    #[allow(dead_code)]
    pub fn new(bucket: String, region: String) -> Self {
        Self {
            _bucket: bucket,
            _region: region,
        }
    }
}

#[async_trait]
impl CheckpointStorage for S3CheckpointStorage {
    async fn store(&self, _checkpoint_id: &str, _checkpoint_path: &Path) -> Result<()> {
        Err(anyhow!("S3 storage not yet implemented"))
    }

    async fn retrieve(&self, _checkpoint_id: &str) -> Result<PathBuf> {
        Err(anyhow!("S3 storage not yet implemented"))
    }

    async fn delete(&self, _checkpoint_id: &str) -> Result<()> {
        Err(anyhow!("S3 storage not yet implemented"))
    }

    async fn list(&self) -> Result<Vec<CheckpointMetadata>> {
        Err(anyhow!("S3 storage not yet implemented"))
    }

    async fn exists(&self, _checkpoint_id: &str) -> Result<bool> {
        Err(anyhow!("S3 storage not yet implemented"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalCheckpointStorage::new(temp_dir.path().to_path_buf()).unwrap();

        // Create a test checkpoint directory
        let checkpoint_dir = temp_dir.path().join("test-checkpoint");
        fs::create_dir(&checkpoint_dir).await.unwrap();
        fs::write(checkpoint_dir.join("test.txt"), b"test data").await.unwrap();

        // Store it
        storage.store("checkpoint-1", &checkpoint_dir).await.unwrap();

        // Verify it exists
        assert!(storage.exists("checkpoint-1").await.unwrap());

        // Retrieve it
        let retrieved = storage.retrieve("checkpoint-1").await.unwrap();
        assert!(retrieved.exists());

        // List checkpoints
        let checkpoints = storage.list().await.unwrap();
        assert_eq!(checkpoints.len(), 1);

        // Delete it
        storage.delete("checkpoint-1").await.unwrap();
        assert!(!storage.exists("checkpoint-1").await.unwrap());
    }
}
