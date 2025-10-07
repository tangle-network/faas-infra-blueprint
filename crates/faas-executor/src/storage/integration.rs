//! Storage integration layer
//! Provides unified snapshot/checkpoint storage across all execution backends

use super::{
    adapters::{DockerSnapshotAdapter, VmSnapshotAdapter},
    tier::LocalBackend,
    BlobCache, BlobStore, TieredStore,
};
use crate::bollard::Docker;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(target_os = "linux")]
use super::adapters::CriuCheckpointAdapter;

/// Unified storage manager for all snapshot/checkpoint types
pub struct StorageManager {
    /// Core blob storage
    blob_store: Arc<BlobStore>,
    /// Cache layer
    cache: Arc<BlobCache>,
    /// Tiered storage (local + remote)
    tiered: Option<Arc<TieredStore>>,
    /// Docker snapshot adapter
    docker_adapter: Arc<DockerSnapshotAdapter>,
    /// Firecracker VM snapshot adapter
    vm_adapter: Arc<VmSnapshotAdapter>,
    /// CRIU checkpoint adapter (Linux only)
    #[cfg(target_os = "linux")]
    criu_adapter: Arc<CriuCheckpointAdapter>,
}

impl StorageManager {
    /// Create new storage manager with blob backend
    pub async fn new(
        base_path: PathBuf,
        docker: Arc<Docker>,
        cache_size_mb: usize,
    ) -> Result<Self> {
        let blob_path = base_path.join("blobs");
        let backend = Arc::new(LocalBackend::new(blob_path).await?);
        let blob_store = Arc::new(BlobStore::new(backend));

        let cache = Arc::new(BlobCache::new(
            blob_store.clone(),
            cache_size_mb,
            10 * 1024 * 1024, // 10MB max blob size in cache
        ));

        let docker_adapter = Arc::new(DockerSnapshotAdapter::new(docker.clone(), cache.clone()));

        let vm_adapter = Arc::new(VmSnapshotAdapter::new(
            cache.clone(),
            base_path.join("firecracker"),
        ));

        #[cfg(target_os = "linux")]
        let criu_adapter = Arc::new(CriuCheckpointAdapter::new(cache.clone()));

        Ok(Self {
            blob_store,
            cache,
            tiered: None,
            docker_adapter,
            vm_adapter,
            #[cfg(target_os = "linux")]
            criu_adapter,
        })
    }

    /// Enable tiered storage with object store backend
    pub fn with_tiered_storage(mut self, object_store_url: Option<String>) -> Result<Self> {
        if let Some(_url) = object_store_url {
            // TODO: Initialize object store backend (S3, MinIO, etc.)
            // For now, keep as local-only
        }
        Ok(self)
    }

    /// Get Docker snapshot adapter
    pub fn docker(&self) -> Arc<DockerSnapshotAdapter> {
        self.docker_adapter.clone()
    }

    /// Get Firecracker VM snapshot adapter
    pub fn vm(&self) -> Arc<VmSnapshotAdapter> {
        self.vm_adapter.clone()
    }

    /// Get CRIU checkpoint adapter (Linux only)
    #[cfg(target_os = "linux")]
    pub fn criu(&self) -> Arc<CriuCheckpointAdapter> {
        self.criu_adapter.clone()
    }

    /// Get blob storage statistics
    pub async fn stats(&self) -> StorageStats {
        // TODO: Implement actual stats collection
        StorageStats {
            total_blobs: 0,
            total_size_bytes: 0,
            cache_hits: 0,
            cache_misses: 0,
            dedup_savings_bytes: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_blobs: usize,
    pub total_size_bytes: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub dedup_savings_bytes: u64,
}
