//! Content-addressed blob storage for snapshots, checkpoints, and caches
//!
//! This module provides a deduplicated storage layer that works across:
//! - CRIU process checkpoints (Linux only)
//! - Firecracker VM snapshots
//! - Docker container images and layers
//!
//! Storage is content-addressed using SHA256 hashing for automatic deduplication.
//! Supports local NVMe storage with optional remote object store backends.

mod adapters;
mod blob;
mod cache;
mod integration;
mod manifest;
mod tier;

#[cfg(target_os = "linux")]
pub use adapters::CriuCheckpointAdapter;
pub use adapters::{DockerSnapshotAdapter, VmSnapshotAdapter, VmSnapshotInfo};
pub use blob::{BlobId, BlobMeta, BlobStore};
pub use cache::BlobCache;
pub use integration::{StorageManager, StorageStats};
pub use manifest::{Manifest, ManifestEntry, ManifestKind};
pub use tier::{StorageTier, TieredStore};

use anyhow::Result;
use async_trait::async_trait;

/// Storage backend trait for different storage implementations
#[async_trait]
pub trait Backend: Send + Sync {
    /// Store a blob, returns content hash
    async fn put(&self, data: &[u8]) -> Result<BlobId>;

    /// Retrieve a blob by content hash
    async fn get(&self, id: &BlobId) -> Result<Vec<u8>>;

    /// Check if blob exists
    async fn exists(&self, id: &BlobId) -> Result<bool>;

    /// Delete a blob (respects reference counting)
    async fn delete(&self, id: &BlobId) -> Result<()>;

    /// Get blob size without fetching content
    async fn size(&self, id: &BlobId) -> Result<u64>;
}

/// Compression format for stored blobs
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum Compression {
    None,
    Zstd,
    Lz4,
}

impl Compression {
    /// Choose compression based on blob size and type
    pub fn choose_for(size: usize, is_executable: bool) -> Self {
        if size < 4096 {
            // Small blobs: no compression overhead
            Self::None
        } else if is_executable || size > 10 * 1024 * 1024 {
            // Large or binary: fast compression
            Self::Lz4
        } else {
            // Text/data: high ratio compression
            Self::Zstd
        }
    }
}
