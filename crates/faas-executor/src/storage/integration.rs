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
    ///
    /// URL formats:
    /// - `s3://bucket` - AWS S3 (uses credentials from environment)
    /// - `s3://bucket/prefix` - AWS S3 with prefix for organization
    /// - `s3://bucket?region=us-east-1` - AWS S3 with specific region
    /// - `s3://bucket?endpoint=https://minio.local:9000` - MinIO or S3-compatible
    /// - `https://minio.example.com/bucket` - Direct HTTPS URL
    ///
    /// Environment variables for authentication:
    /// - AWS_ACCESS_KEY_ID
    /// - AWS_SECRET_ACCESS_KEY
    /// - AWS_REGION (optional, defaults from URL)
    /// - AWS_ENDPOINT (optional, for S3-compatible services)
    #[cfg(feature = "object-storage")]
    pub async fn with_tiered_storage_async(
        mut self,
        object_store_url: Option<String>,
    ) -> Result<Self> {
        use super::tier::{ObjectBackend, TieredStore};
        use super::Backend;
        use anyhow::Context;
        use tracing::info;

        if let Some(url) = object_store_url {
            info!("Configuring tiered storage with object store: {}", url);

            // Create object backend from URL
            let object_backend = Arc::new(
                ObjectBackend::from_url(&url)
                    .await
                    .context("Failed to create object store backend")?,
            );

            // Get the base path from current blob_store backend
            let base_path = if cfg!(target_os = "linux") {
                std::path::PathBuf::from("/var/lib/faas/blobs")
            } else {
                std::env::temp_dir().join("faas").join("blobs")
            };

            // Create tiered store (local + remote)
            let tiered = TieredStore::new(base_path)
                .await?
                .with_object_store(object_backend);

            // Replace blob_store backend with tiered store
            self.blob_store = Arc::new(BlobStore::new(Arc::new(tiered) as Arc<dyn Backend>));

            // Recreate cache with new blob_store
            self.cache = Arc::new(BlobCache::new(
                self.blob_store.clone(),
                100,              // cache size in entries
                10 * 1024 * 1024, // 10MB max blob size in cache
            ));

            // Update adapters to use new cache
            self.docker_adapter = Arc::new(DockerSnapshotAdapter::new(
                self.docker_adapter.docker.clone(),
                self.cache.clone(),
            ));

            self.vm_adapter = Arc::new(VmSnapshotAdapter::new(
                self.cache.clone(),
                self.vm_adapter.snapshot_dir.clone(),
            ));

            #[cfg(target_os = "linux")]
            {
                self.criu_adapter = Arc::new(CriuCheckpointAdapter::new(self.cache.clone()));
            }

            info!("Tiered storage enabled: local NVMe + object store");
        }

        Ok(self)
    }

    /// Stub when object-storage feature is disabled
    #[cfg(not(feature = "object-storage"))]
    pub async fn with_tiered_storage_async(self, object_store_url: Option<String>) -> Result<Self> {
        if object_store_url.is_some() {
            return Err(anyhow::anyhow!(
                "Object storage not enabled. Rebuild with --features object-storage"
            ));
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
