//! Multi-tier storage: Local NVMe → Object Store → CDN

use super::{Backend, BlobId};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

/// Storage tier type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageTier {
    /// Local NVMe (fastest)
    Local,
    /// Object store (S3/MinIO)
    Object,
    /// CDN edge cache (read-only)
    Cdn,
}

/// Local filesystem backend (NVMe optimized)
pub struct LocalBackend {
    root: PathBuf,
}

impl LocalBackend {
    pub async fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root).await?;
        Ok(Self { root })
    }
}

#[async_trait]
impl Backend for LocalBackend {
    async fn put(&self, data: &[u8]) -> Result<BlobId> {
        let id = BlobId::from_bytes(data);
        let path = id.path(&self.root);

        // Create parent directory
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write atomically
        let temp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(data).await?;
        file.sync_all().await?;
        drop(file);

        fs::rename(temp_path, path).await?;

        debug!("Stored blob {} ({} bytes) to local", id.as_str(), data.len());
        Ok(id)
    }

    async fn get(&self, id: &BlobId) -> Result<Vec<u8>> {
        let path = id.path(&self.root);
        fs::read(&path).await
            .map_err(|e| anyhow!("Failed to read blob {}: {}", id.as_str(), e))
    }

    async fn exists(&self, id: &BlobId) -> Result<bool> {
        Ok(id.path(&self.root).exists())
    }

    async fn delete(&self, id: &BlobId) -> Result<()> {
        let path = id.path(&self.root);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    async fn size(&self, id: &BlobId) -> Result<u64> {
        let path = id.path(&self.root);
        let metadata = fs::metadata(&path).await?;
        Ok(metadata.len())
    }
}

/// Object store backend (S3/MinIO compatible)
pub struct ObjectBackend {
    _bucket: String,
    _endpoint: String,
}

impl ObjectBackend {
    #[allow(dead_code)]
    pub fn new(bucket: String, endpoint: String) -> Self {
        Self {
            _bucket: bucket,
            _endpoint: endpoint,
        }
    }
}

#[async_trait]
impl Backend for ObjectBackend {
    async fn put(&self, _data: &[u8]) -> Result<BlobId> {
        Err(anyhow!("Object store backend not yet implemented"))
    }

    async fn get(&self, _id: &BlobId) -> Result<Vec<u8>> {
        Err(anyhow!("Object store backend not yet implemented"))
    }

    async fn exists(&self, _id: &BlobId) -> Result<bool> {
        Ok(false)
    }

    async fn delete(&self, _id: &BlobId) -> Result<()> {
        Ok(())
    }

    async fn size(&self, _id: &BlobId) -> Result<u64> {
        Ok(0)
    }
}

/// Multi-tier storage with automatic promotion/demotion
pub struct TieredStore {
    local: Arc<LocalBackend>,
    object: Option<Arc<ObjectBackend>>,
}

impl TieredStore {
    pub async fn new(local_root: PathBuf) -> Result<Self> {
        let local = Arc::new(LocalBackend::new(local_root).await?);
        Ok(Self {
            local,
            object: None,
        })
    }

    pub fn with_object_store(mut self, object: Arc<ObjectBackend>) -> Self {
        self.object = Some(object);
        self
    }

    /// Get from fastest available tier
    async fn get_from_tiers(&self, id: &BlobId) -> Result<Vec<u8>> {
        // Try local first
        if self.local.exists(id).await? {
            return self.local.get(id).await;
        }

        // Try object store
        if let Some(object) = &self.object {
            if object.exists(id).await? {
                let data = object.get(id).await?;
                // Promote to local cache
                self.local.put(&data).await?;
                info!("Promoted blob {} from object store to local", id.as_str());
                return Ok(data);
            }
        }

        Err(anyhow!("Blob {} not found in any tier", id.as_str()))
    }
}

#[async_trait]
impl Backend for TieredStore {
    async fn put(&self, data: &[u8]) -> Result<BlobId> {
        let id = self.local.put(data).await?;

        // Async push to object store if configured
        if let Some(object) = &self.object {
            let object = object.clone();
            let data = data.to_vec();
            tokio::spawn(async move {
                if let Err(e) = object.put(&data).await {
                    debug!("Failed to replicate blob to object store: {}", e);
                }
            });
        }

        Ok(id)
    }

    async fn get(&self, id: &BlobId) -> Result<Vec<u8>> {
        self.get_from_tiers(id).await
    }

    async fn exists(&self, id: &BlobId) -> Result<bool> {
        if self.local.exists(id).await? {
            return Ok(true);
        }

        if let Some(object) = &self.object {
            return object.exists(id).await;
        }

        Ok(false)
    }

    async fn delete(&self, id: &BlobId) -> Result<()> {
        self.local.delete(id).await?;

        if let Some(object) = &self.object {
            object.delete(id).await?;
        }

        Ok(())
    }

    async fn size(&self, id: &BlobId) -> Result<u64> {
        if self.local.exists(id).await? {
            return self.local.size(id).await;
        }

        if let Some(object) = &self.object {
            return object.size(id).await;
        }

        Err(anyhow!("Blob {} not found", id.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_backend() {
        let temp = TempDir::new().unwrap();
        let backend = LocalBackend::new(temp.path().to_path_buf()).await.unwrap();

        let data = b"test data";
        let id = backend.put(data).await.unwrap();

        assert!(backend.exists(&id).await.unwrap());
        assert_eq!(backend.get(&id).await.unwrap(), data);
        assert_eq!(backend.size(&id).await.unwrap(), data.len() as u64);
    }
}
