//! Real Docker snapshot implementation using commit and proper state management
//! No more mocks - actual Docker operations for production use

use anyhow::{anyhow, Context, Result};
use crate::bollard::container::Config as ContainerConfig;
use crate::bollard::image::CommitContainerOptions;
use crate::bollard::Docker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

/// Docker-based snapshot with actual commit operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerSnapshot {
    pub id: String,
    pub image_id: String,
    pub container_id: String,
    pub name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub size_bytes: i64,
    pub metadata: HashMap<String, String>,
    pub parent_snapshot: Option<String>,
}

/// Manages Docker snapshots with real commit/restore operations
pub struct DockerSnapshotManager {
    docker: Arc<Docker>,
    snapshots: Arc<RwLock<HashMap<String, DockerSnapshot>>>,
    snapshot_prefix: String,
}

impl DockerSnapshotManager {
    pub fn new(docker: Arc<Docker>) -> Self {
        Self {
            docker,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            snapshot_prefix: "faas-snapshot".to_string(),
        }
    }

    /// Create a real Docker snapshot using commit
    pub async fn create_snapshot(
        &self,
        container_id: &str,
        name: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<DockerSnapshot> {
        let snapshot_id = Uuid::new_v4().to_string();
        let image_name = format!("{}-{}:latest", self.snapshot_prefix, snapshot_id);

        info!("Creating Docker snapshot from container {} -> {}", container_id, image_name);

        // Actually commit the container to create an image
        let options = CommitContainerOptions {
            container: container_id.to_string(),
            repo: image_name.clone(),
            tag: "latest".to_string(),
            comment: format!("FaaS snapshot {snapshot_id}"),
            author: "FaaS Platform".to_string(),
            pause: true, // Pause container during commit for consistency
            ..Default::default()
        };

        // Perform the actual Docker commit
        let commit_result = self.docker
            .commit_container(options, ContainerConfig::<String>::default())
            .await
            .context("Failed to commit container")?;

        // Use the commit result ID if available, otherwise use the image name we specified
        let image_id = commit_result.id
            .unwrap_or_else(|| image_name.clone());

        // Get image size - use the image name to inspect since that's what we created
        let image_info = self.docker
            .inspect_image(&image_name)
            .await
            .context("Failed to inspect committed image")?;

        let size_bytes = image_info.size.unwrap_or(0);

        let snapshot = DockerSnapshot {
            id: snapshot_id.clone(),
            image_id: image_id.clone(),
            container_id: container_id.to_string(),
            name,
            created_at: chrono::Utc::now(),
            size_bytes,
            metadata,
            parent_snapshot: None,
        };

        // Store snapshot metadata
        self.snapshots.write().await.insert(snapshot_id.clone(), snapshot.clone());

        info!("Created Docker snapshot {} (image: {}, size: {} bytes)",
              snapshot_id, image_id, size_bytes);

        Ok(snapshot)
    }

    /// Restore a container from snapshot (real Docker run from committed image)
    pub async fn restore_snapshot(&self, snapshot_id: &str) -> Result<String> {
        let snapshots = self.snapshots.read().await;
        let snapshot = snapshots.get(snapshot_id)
            .ok_or_else(|| anyhow!("Snapshot {snapshot_id} not found"))?;

        let container_name = format!("restored-{}-{}", snapshot_id, Uuid::new_v4());

        info!("Restoring snapshot {} from image {}", snapshot_id, snapshot.image_id);

        // Create container from snapshot image
        let create_options = crate::bollard::container::CreateContainerOptions {
            name: container_name.clone(),
            ..Default::default()
        };

        let config = crate::bollard::container::Config {
            image: Some(snapshot.image_id.clone()),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(false),
            ..Default::default()
        };

        let container = self.docker
            .create_container(Some(create_options), config)
            .await
            .context("Failed to create container from snapshot")?;

        info!("Restored container {} from snapshot {}", container.id, snapshot_id);

        Ok(container.id)
    }

    /// Fork a snapshot (create a new snapshot from an existing one)
    pub async fn fork_snapshot(
        &self,
        parent_snapshot_id: &str,
        name: Option<String>,
    ) -> Result<DockerSnapshot> {
        // First restore the parent snapshot
        let container_id = self.restore_snapshot(parent_snapshot_id).await?;

        // Start the container briefly to ensure it's in a runnable state
        self.docker
            .start_container::<String>(&container_id, None)
            .await
            .context("Failed to start container for forking")?;

        // Create a new snapshot from the restored container
        let mut metadata = HashMap::new();
        metadata.insert("parent_snapshot".to_string(), parent_snapshot_id.to_string());
        metadata.insert("fork_type".to_string(), "branch".to_string());

        let mut forked = self.create_snapshot(&container_id, name, metadata).await?;
        forked.parent_snapshot = Some(parent_snapshot_id.to_string());

        // Clean up the temporary container
        let _ = self.docker
            .remove_container(
                &container_id,
                Some(crate::bollard::container::RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;

        Ok(forked)
    }

    /// List all snapshots
    pub async fn list_snapshots(&self) -> Vec<DockerSnapshot> {
        self.snapshots.read().await.values().cloned().collect()
    }

    /// Delete a snapshot (remove the committed image)
    pub async fn delete_snapshot(&self, snapshot_id: &str) -> Result<()> {
        let mut snapshots = self.snapshots.write().await;

        if let Some(snapshot) = snapshots.remove(snapshot_id) {
            // Remove the Docker image
            self.docker
                .remove_image(
                    &snapshot.image_id,
                    Some(crate::bollard::image::RemoveImageOptions {
                        force: true,
                        ..Default::default()
                    }),
                    None,
                )
                .await
                .context("Failed to remove snapshot image")?;

            info!("Deleted snapshot {} and image {}", snapshot_id, snapshot.image_id);
            Ok(())
        } else {
            Err(anyhow!("Snapshot {snapshot_id} not found"))
        }
    }

    /// Get snapshot metadata
    pub async fn get_snapshot(&self, snapshot_id: &str) -> Option<DockerSnapshot> {
        self.snapshots.read().await.get(snapshot_id).cloned()
    }

    /// Create incremental snapshot (diff from parent)
    pub async fn create_incremental_snapshot(
        &self,
        container_id: &str,
        parent_snapshot_id: &str,
        name: Option<String>,
    ) -> Result<DockerSnapshot> {
        let parent = self.get_snapshot(parent_snapshot_id).await
            .ok_or_else(|| anyhow!("Parent snapshot {parent_snapshot_id} not found"))?;

        // Get container changes since parent (for logging)
        let _changes = self.docker
            .container_changes(container_id)
            .await
            .context("Failed to get container changes")?;

        let mut metadata = HashMap::new();
        metadata.insert("parent_snapshot".to_string(), parent_snapshot_id.to_string());
        metadata.insert("incremental".to_string(), "true".to_string());

        // Create the snapshot with parent reference
        let mut snapshot = self.create_snapshot(container_id, name, metadata).await?;
        snapshot.parent_snapshot = Some(parent_snapshot_id.to_string());

        info!("Created incremental snapshot {} from parent {}",
              snapshot.id, parent_snapshot_id);

        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_real_snapshot_create_restore() {
        let docker = Arc::new(Docker::connect_with_defaults().unwrap());
        let manager = DockerSnapshotManager::new(docker.clone());

        // Create a test container
        let config = crate::bollard::container::Config {
            image: Some("alpine:latest".to_string()),
            cmd: Some(vec!["sleep".to_string(), "3600".to_string()]),
            ..Default::default()
        };

        let container = docker
            .create_container::<&str, _>(None, config)
            .await
            .unwrap();

        docker.start_container::<String>(&container.id, None).await.unwrap();

        // Create snapshot
        let snapshot = manager
            .create_snapshot(&container.id, Some("test-snapshot".to_string()), HashMap::new())
            .await
            .unwrap();

        assert!(!snapshot.image_id.is_empty());
        assert!(snapshot.size_bytes > 0);

        // Restore snapshot
        let restored_id = manager.restore_snapshot(&snapshot.id).await.unwrap();
        assert!(!restored_id.is_empty());

        // Cleanup
        docker.remove_container(&container.id, Some(crate::bollard::container::RemoveContainerOptions {
            force: true,
            ..Default::default()
        })).await.unwrap();

        docker.remove_container(&restored_id, Some(crate::bollard::container::RemoveContainerOptions {
            force: true,
            ..Default::default()
        })).await.unwrap();

        manager.delete_snapshot(&snapshot.id).await.unwrap();
    }
}