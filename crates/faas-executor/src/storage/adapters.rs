//! Backward-compatible adapters for existing snapshot systems
//! Allows gradual migration to blob storage without breaking existing code

use super::{BlobCache, Compression, Manifest, ManifestKind};
use crate::bollard::Docker;
use anyhow::{anyhow, Context, Result};
use firecracker_rs_sdk::instance::Instance as FcInstance;
use futures::TryStreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::info;

/// Adapter for Docker snapshots using blob storage backend
pub struct DockerSnapshotAdapter {
    docker: Arc<Docker>,
    cache: Arc<BlobCache>,
    manifests: Arc<RwLock<HashMap<String, Manifest>>>,
}

impl DockerSnapshotAdapter {
    pub fn new(docker: Arc<Docker>, cache: Arc<BlobCache>) -> Self {
        Self {
            docker,
            cache,
            manifests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create snapshot using blob storage (replaces DockerSnapshotManager::create_snapshot)
    pub async fn create_snapshot(
        &self,
        container_id: &str,
        name: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<String> {
        info!("Creating Docker snapshot via blob adapter: {}", container_id);

        // Export container filesystem
        let export_stream = self.docker.export_container(container_id);
        let tar_data = self
            .collect_stream(export_stream)
            .await
            .context("Failed to export container")?;

        // Store as deduplicated blob
        let compression = Compression::choose_for(tar_data.len(), false);
        let blob_id = self
            .cache
            .put(&tar_data, compression)
            .await
            .context("Failed to store container blob")?;

        // Create manifest
        let snapshot_id = uuid::Uuid::new_v4().to_string();
        let mut manifest = Manifest::new(
            snapshot_id.clone(),
            ManifestKind::DockerLayers {
                container_id: container_id.to_string(),
                base_image: self.get_container_image(container_id).await?,
            },
        );

        manifest.add_entry(
            "filesystem.tar".to_string(),
            blob_id,
            tar_data.len() as u64,
            None,
        );

        // Add custom metadata
        for (k, v) in metadata {
            manifest.metadata.insert(k, v);
        }
        if let Some(n) = name {
            manifest.metadata.insert("name".to_string(), n);
        }

        // Store manifest
        self.manifests.write().await.insert(snapshot_id.clone(), manifest);

        info!("Docker snapshot created: {} ({} bytes)", snapshot_id, tar_data.len());
        Ok(snapshot_id)
    }

    /// Restore snapshot (replaces DockerSnapshotManager::restore_snapshot)
    pub async fn restore_snapshot(&self, snapshot_id: &str) -> Result<String> {
        info!("Restoring Docker snapshot: {}", snapshot_id);

        let manifests = self.manifests.read().await;
        let manifest = manifests
            .get(snapshot_id)
            .ok_or_else(|| anyhow!("Snapshot not found: {}", snapshot_id))?;

        // Get the filesystem blob
        let fs_entry = manifest
            .entries
            .iter()
            .find(|e| e.path == "filesystem.tar")
            .ok_or_else(|| anyhow!("Filesystem blob not found in manifest"))?;

        let tar_data = self.cache.get(&fs_entry.blob_id).await?;

        // Import as Docker image
        let image_name = format!("faas-restored-{}", snapshot_id);
        self.docker
            .import_image(
                crate::bollard::image::ImportImageOptions {
                    ..Default::default()
                },
                tar_data.into(),
                None,
            )
            .try_collect::<Vec<_>>()
            .await
            .context("Failed to import image")?;

        // Create container from imported image
        let container_name = format!("restored-{}", snapshot_id);
        let config = crate::bollard::container::Config {
            image: Some(image_name),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let container = self
            .docker
            .create_container(
                Some(crate::bollard::container::CreateContainerOptions {
                    name: container_name,
                    ..Default::default()
                }),
                config,
            )
            .await
            .context("Failed to create container from snapshot")?;

        info!("Restored Docker container: {}", container.id);
        Ok(container.id)
    }

    async fn get_container_image(&self, container_id: &str) -> Result<String> {
        let inspect = self.docker.inspect_container(container_id, None).await?;
        Ok(inspect
            .config
            .and_then(|c| c.image)
            .unwrap_or_else(|| "unknown".to_string()))
    }

    async fn collect_stream<S, T, E>(&self, stream: S) -> Result<Vec<u8>>
    where
        S: futures::Stream<Item = Result<T, E>>,
        T: AsRef<[u8]>,
        E: std::error::Error + Send + Sync + 'static,
    {
        use futures::StreamExt;
        let mut data = Vec::new();
        let mut stream = Box::pin(stream);
        while let Some(chunk) = stream.next().await {
            data.extend_from_slice(chunk?.as_ref());
        }
        Ok(data)
    }
}

/// Adapter for Firecracker VM snapshots using blob storage
pub struct VmSnapshotAdapter {
    cache: Arc<BlobCache>,
    manifests: Arc<RwLock<HashMap<String, Manifest>>>,
    snapshot_dir: PathBuf,
}

impl VmSnapshotAdapter {
    pub fn new(cache: Arc<BlobCache>, snapshot_dir: PathBuf) -> Self {
        Self {
            cache,
            manifests: Arc::new(RwLock::new(HashMap::new())),
            snapshot_dir,
        }
    }

    /// Create VM snapshot (replaces VmSnapshotManager::create_snapshot)
    pub async fn create_snapshot(
        &self,
        vm_id: &str,
        snapshot_id: &str,
        fc_instance: &mut FcInstance,
    ) -> Result<VmSnapshotInfo> {
        info!("Creating Firecracker snapshot via blob adapter: {}", snapshot_id);

        let snapshot_path = self.snapshot_dir.join(snapshot_id);
        tokio::fs::create_dir_all(&snapshot_path).await?;

        let memory_file = snapshot_path.join("memory.snap");
        let state_file = snapshot_path.join("state.snap");

        #[cfg(target_os = "linux")]
        {
            use firecracker_rs_sdk::models::{SnapshotCreateParams, SnapshotType};

            let params = SnapshotCreateParams {
                snapshot_type: Some(SnapshotType::Full),
                snapshot_path: state_file.clone(),
                mem_file_path: memory_file.clone(),
                version: Some("1.0.0".to_string()),
            };

            fc_instance
                .create_snapshot(&params)
                .map_err(|e| anyhow!("Failed to create VM snapshot: {:?}", e))?;
        }

        #[cfg(not(target_os = "linux"))]
        {
            tokio::fs::write(&memory_file, b"MEMORY_PLACEHOLDER").await?;
            tokio::fs::write(&state_file, b"STATE_PLACEHOLDER").await?;
        }

        // Read snapshot files and store as blobs
        let memory_data = tokio::fs::read(&memory_file).await?;
        let state_data = tokio::fs::read(&state_file).await?;

        let memory_compression = Compression::choose_for(memory_data.len(), true);
        let state_compression = Compression::choose_for(state_data.len(), false);

        let memory_blob = self.cache.put(&memory_data, memory_compression).await?;
        let state_blob = self.cache.put(&state_data, state_compression).await?;

        // Create manifest
        let mut manifest = Manifest::new(
            snapshot_id.to_string(),
            ManifestKind::FirecrackerSnapshot {
                vm_id: vm_id.to_string(),
                memory_blob: memory_blob.clone(),
                state_blob: state_blob.clone(),
            },
        );

        manifest.add_entry(
            "memory.snap".to_string(),
            memory_blob,
            memory_data.len() as u64,
            None,
        );
        manifest.add_entry(
            "state.snap".to_string(),
            state_blob,
            state_data.len() as u64,
            None,
        );

        let total_size = memory_data.len() + state_data.len();

        self.manifests.write().await.insert(snapshot_id.to_string(), manifest);

        info!("Firecracker snapshot created: {} ({} bytes)", snapshot_id, total_size);

        Ok(VmSnapshotInfo {
            id: snapshot_id.to_string(),
            vm_id: vm_id.to_string(),
            size_bytes: total_size as u64,
            created_at: SystemTime::now(),
        })
    }

    /// Restore VM snapshot (replaces VmSnapshotManager::restore_snapshot)
    pub async fn restore_snapshot(&self, snapshot_id: &str, new_vm_id: &str) -> Result<PathBuf> {
        info!("Restoring Firecracker snapshot: {} -> {}", snapshot_id, new_vm_id);

        let manifests = self.manifests.read().await;
        let manifest = manifests
            .get(snapshot_id)
            .ok_or_else(|| anyhow!("Snapshot not found: {}", snapshot_id))?;

        // Extract blob IDs from manifest
        let (memory_blob, state_blob) = match &manifest.kind {
            ManifestKind::FirecrackerSnapshot {
                memory_blob,
                state_blob,
                ..
            } => (memory_blob, state_blob),
            _ => return Err(anyhow!("Invalid manifest type for Firecracker snapshot")),
        };

        // Retrieve blobs
        let memory_data = self.cache.get(memory_blob).await?;
        let state_data = self.cache.get(state_blob).await?;

        // Write to restore directory
        let restore_dir = self.snapshot_dir.join("restore").join(new_vm_id);
        tokio::fs::create_dir_all(&restore_dir).await?;

        let memory_file = restore_dir.join("memory.snap");
        let state_file = restore_dir.join("state.snap");

        tokio::fs::write(&memory_file, memory_data).await?;
        tokio::fs::write(&state_file, state_data).await?;

        info!("Firecracker snapshot restored to: {:?}", restore_dir);
        Ok(restore_dir)
    }
}

/// CRIU checkpoint adapter using blob storage
#[cfg(target_os = "linux")]
pub struct CriuCheckpointAdapter {
    cache: Arc<BlobCache>,
    manifests: Arc<RwLock<HashMap<String, Manifest>>>,
}

#[cfg(target_os = "linux")]
impl CriuCheckpointAdapter {
    pub fn new(cache: Arc<BlobCache>) -> Self {
        Self {
            cache,
            manifests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store CRIU checkpoint directory as blobs
    pub async fn store_checkpoint(&self, checkpoint_id: &str, images_dir: &PathBuf, pid: u32) -> Result<()> {
        info!("Storing CRIU checkpoint via blob adapter: {}", checkpoint_id);

        let mut manifest = Manifest::new(
            checkpoint_id.to_string(),
            ManifestKind::CriuCheckpoint {
                pid,
                images_dir: images_dir.to_string_lossy().to_string(),
            },
        );

        // Scan checkpoint directory and store each file as blob
        let mut entries = tokio::fs::read_dir(images_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                let data = tokio::fs::read(&path).await?;
                let compression = Compression::choose_for(data.len(), false);
                let blob_id = self.cache.put(&data, compression).await?;

                let file_name = path
                    .file_name()
                    .ok_or_else(|| anyhow!("Invalid file name"))?
                    .to_string_lossy()
                    .to_string();

                manifest.add_entry(file_name, blob_id, data.len() as u64, None);
            }
        }

        self.manifests.write().await.insert(checkpoint_id.to_string(), manifest);

        info!("CRIU checkpoint stored: {} ({} files)", checkpoint_id, manifest.entries.len());
        Ok(())
    }

    /// Restore CRIU checkpoint from blobs
    pub async fn restore_checkpoint(&self, checkpoint_id: &str, restore_dir: &PathBuf) -> Result<()> {
        info!("Restoring CRIU checkpoint: {}", checkpoint_id);

        let manifests = self.manifests.read().await;
        let manifest = manifests
            .get(checkpoint_id)
            .ok_or_else(|| anyhow!("Checkpoint not found: {}", checkpoint_id))?;

        tokio::fs::create_dir_all(restore_dir).await?;

        // Restore each file from blobs
        for entry in &manifest.entries {
            let data = self.cache.get(&entry.blob_id).await?;
            let file_path = restore_dir.join(&entry.path);
            tokio::fs::write(&file_path, data).await?;
        }

        info!("CRIU checkpoint restored: {} ({} files)", checkpoint_id, manifest.entries.len());
        Ok(())
    }
}

/// Snapshot info returned by adapters
#[derive(Debug, Clone)]
pub struct VmSnapshotInfo {
    pub id: String,
    pub vm_id: String,
    pub size_bytes: u64,
    pub created_at: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_vm_snapshot_adapter() {
        let temp_dir = tempdir().unwrap();
        let store = Arc::new(BlobStore::new(temp_dir.path().join("blobs")).unwrap());
        let cache = Arc::new(BlobCache::new(store, 100, 10 * 1024 * 1024));

        let adapter = VmSnapshotAdapter::new(cache, temp_dir.path().join("snapshots"));

        // Note: Real FC instance testing requires Linux and actual Firecracker
        // This is a placeholder for the adapter structure
    }
}
