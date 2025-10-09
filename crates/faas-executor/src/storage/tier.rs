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

/// S3-compatible object store backend (AWS S3, MinIO, R2, DO Spaces, etc.)
#[cfg(feature = "object-storage")]
pub struct ObjectBackend {
    store: Arc<dyn object_store::ObjectStore>,
    prefix: object_store::path::Path,
}

#[cfg(feature = "object-storage")]
impl ObjectBackend {
    /// Create from S3-compatible URL (s3://bucket/prefix, https://minio.example.com/bucket)
    pub async fn from_url(url: &str) -> Result<Self> {
        use object_store::{parse_url, ClientOptions};

        let parsed_url = url::Url::parse(url)
            .map_err(|e| anyhow!("Invalid object store URL: {e}"))?;

        let (store, path) = parse_url(&parsed_url)
            .map_err(|e| anyhow!("Failed to parse object store URL: {e}"))?;

        Ok(Self {
            store: Arc::from(store),
            prefix: path,
        })
    }

    /// Create with explicit S3 configuration
    pub fn new_s3(bucket: String, region: String, endpoint: Option<String>) -> Result<Self> {
        use object_store::aws::{AmazonS3Builder, AmazonS3ConfigKey};

        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(&bucket)
            .with_region(&region);

        if let Some(endpoint) = endpoint {
            builder = builder.with_endpoint(&endpoint);
        }

        // Allow anonymous access or credentials from environment
        builder = builder.with_allow_http(true);

        let store = builder.build()
            .map_err(|e| anyhow!("Failed to create S3 backend: {e}"))?;

        Ok(Self {
            store: Arc::new(store),
            prefix: object_store::path::Path::from(""),
        })
    }

    fn blob_path(&self, id: &BlobId) -> object_store::path::Path {
        // Git-style: first 2 chars as dir, rest as file
        let hash = id.as_str();
        let (dir, file) = hash.split_at(2.min(hash.len()));
        self.prefix.child(dir).child(file)
    }
}

#[cfg(feature = "object-storage")]
#[async_trait]
impl Backend for ObjectBackend {
    async fn put(&self, data: &[u8]) -> Result<BlobId> {
        use bytes::Bytes;
        use object_store::PutPayload;

        let id = BlobId::from_bytes(data);
        let path = self.blob_path(&id);

        let payload = PutPayload::from(Bytes::copy_from_slice(data));
        self.store.put(&path, payload)
            .await
            .map_err(|e| anyhow!("Failed to put blob to object store: {e}"))?;

        debug!("Stored blob {} to object store at {}", id.as_str(), path);
        Ok(id)
    }

    async fn get(&self, id: &BlobId) -> Result<Vec<u8>> {
        use object_store::GetResult;

        let path = self.blob_path(id);

        let result = self.store.get(&path)
            .await
            .map_err(|e| anyhow!("Failed to get blob from object store: {e}"))?;

        let bytes = result.bytes()
            .await
            .map_err(|e| anyhow!("Failed to read blob bytes: {e}"))?;

        Ok(bytes.to_vec())
    }

    async fn exists(&self, id: &BlobId) -> Result<bool> {
        let path = self.blob_path(id);

        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(anyhow!("Failed to check blob existence: {e}")),
        }
    }

    async fn delete(&self, id: &BlobId) -> Result<()> {
        let path = self.blob_path(id);

        self.store.delete(&path)
            .await
            .map_err(|e| anyhow!("Failed to delete blob from object store: {e}"))?;

        Ok(())
    }

    async fn size(&self, id: &BlobId) -> Result<u64> {
        let path = self.blob_path(id);

        let meta = self.store.head(&path)
            .await
            .map_err(|e| anyhow!("Failed to get blob metadata: {e}"))?;

        Ok(meta.size as u64)
    }
}

/// Stub ObjectBackend when feature is disabled
#[cfg(not(feature = "object-storage"))]
pub struct ObjectBackend {
    _bucket: String,
}

#[cfg(not(feature = "object-storage"))]
impl ObjectBackend {
    #[allow(dead_code)]
    pub fn new(_bucket: String, _region: String, _endpoint: Option<String>) -> Result<Self> {
        Err(anyhow!("Object storage feature not enabled. Enable with --features object-storage"))
    }
}

#[cfg(not(feature = "object-storage"))]
#[async_trait]
impl Backend for ObjectBackend {
    async fn put(&self, _data: &[u8]) -> Result<BlobId> {
        Err(anyhow!("Object storage not enabled"))
    }

    async fn get(&self, _id: &BlobId) -> Result<Vec<u8>> {
        Err(anyhow!("Object storage not enabled"))
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
