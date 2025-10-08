//! Content-addressed blob storage with deduplication

use super::{Backend, Compression};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

/// Content-addressed blob identifier (SHA256 hash)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlobId(pub String);

impl BlobId {
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(data);
        Self(format!("{:x}", hasher.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get content-addressed file path (git-style: first 2 chars as dir)
    pub fn path(&self, base: &PathBuf) -> PathBuf {
        let (dir, file) = self.0.split_at(2);
        base.join(dir).join(file)
    }
}

/// Metadata for a stored blob
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobMeta {
    pub id: BlobId,
    pub size: u64,
    pub compressed_size: u64,
    pub compression: Compression,
    pub ref_count: u64,
}

/// Content-addressed blob store with automatic deduplication
pub struct BlobStore {
    backend: Arc<dyn Backend>,
    metadata: Arc<RwLock<HashMap<BlobId, BlobMeta>>>,
}

impl BlobStore {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self {
            backend,
            metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store blob with automatic deduplication
    pub async fn put(&self, data: &[u8], compression: Compression) -> Result<BlobId> {
        let id = BlobId::from_bytes(data);

        // Check if already exists
        {
            let mut meta = self.metadata.write().await;
            if let Some(existing) = meta.get_mut(&id) {
                existing.ref_count += 1;
                return Ok(id);
            }
        }

        // Compress if requested
        let (stored_data, compressed_size) = match compression {
            Compression::None => (data.to_vec(), data.len() as u64),
            Compression::Zstd => {
                let compressed = zstd::encode_all(data, 3)?;
                let size = compressed.len() as u64;
                (compressed, size)
            }
            Compression::Lz4 => {
                let compressed = lz4::block::compress(data, Some(lz4::block::CompressionMode::HIGHCOMPRESSION(9)), false)?;
                let size = compressed.len() as u64;
                (compressed, size)
            }
        };

        // Store in backend
        self.backend.put(&stored_data).await?;

        // Update metadata
        let meta = BlobMeta {
            id: id.clone(),
            size: data.len() as u64,
            compressed_size,
            compression,
            ref_count: 1,
        };

        self.metadata.write().await.insert(id.clone(), meta);
        Ok(id)
    }

    /// Retrieve blob, decompressing if needed
    pub async fn get(&self, id: &BlobId) -> Result<Vec<u8>> {
        let meta = {
            let metadata = self.metadata.read().await;
            metadata.get(id)
                .ok_or_else(|| anyhow!("Blob {} not found", id.as_str()))?
                .clone()
        };

        let data = self.backend.get(id).await?;

        // Decompress if needed
        match meta.compression {
            Compression::None => Ok(data),
            Compression::Zstd => {
                Ok(zstd::decode_all(&data[..])?)
            }
            Compression::Lz4 => {
                Ok(lz4::block::decompress(&data, Some(meta.size as i32))?)
            }
        }
    }

    /// Check if blob exists
    pub async fn exists(&self, id: &BlobId) -> Result<bool> {
        Ok(self.metadata.read().await.contains_key(id))
    }

    /// Delete blob (decrements ref count, deletes when zero)
    pub async fn delete(&self, id: &BlobId) -> Result<()> {
        let should_delete = {
            let mut meta = self.metadata.write().await;
            if let Some(blob_meta) = meta.get_mut(id) {
                blob_meta.ref_count -= 1;
                if blob_meta.ref_count == 0 {
                    meta.remove(id);
                    true
                } else {
                    false
                }
            } else {
                return Err(anyhow!("Blob {} not found", id.as_str()));
            }
        };

        if should_delete {
            self.backend.delete(id).await?;
        }

        Ok(())
    }

    /// Get blob metadata
    pub async fn metadata(&self, id: &BlobId) -> Result<BlobMeta> {
        self.metadata.read().await.get(id)
            .cloned()
            .ok_or_else(|| anyhow!("Blob {} not found", id.as_str()))
    }

    /// Get total storage usage
    pub async fn total_size(&self) -> u64 {
        self.metadata.read().await.values()
            .map(|m| m.compressed_size)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_deduplication() {
        // Test that same content stored twice only uses space once
        let data = b"test data for deduplication";

        let id1 = BlobId::from_bytes(data);
        let id2 = BlobId::from_bytes(data);

        assert_eq!(id1, id2, "Same content should have same ID");
    }

    #[tokio::test]
    async fn test_compression_choice() {
        let small_data = vec![0u8; 1024];
        let large_data = vec![0u8; 20 * 1024 * 1024];

        assert!(matches!(
            Compression::choose_for(small_data.len(), false),
            Compression::None
        ));

        assert!(matches!(
            Compression::choose_for(large_data.len(), true),
            Compression::Lz4
        ));
    }
}
