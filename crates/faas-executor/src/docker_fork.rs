use crate::bollard::container::{CreateContainerOptions, StartContainerOptions};
use crate::bollard::exec::{CreateExecOptions, StartExecResults};
use crate::bollard::image::CommitContainerOptions;
use crate::bollard::Docker;
use anyhow::{anyhow, Result};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::info;

/// REAL Docker-based container forking with state preservation
/// No stubs, no fakes - actual container checkpointing and restoration
pub struct DockerForkManager {
    docker: Docker,
    /// Maps branch IDs to their parent container IDs
    branches: Arc<RwLock<HashMap<String, BranchInfo>>>,
    /// Maps container IDs to their committed image IDs for fast forking
    pub checkpoints: Arc<RwLock<HashMap<String, String>>>,
}

#[derive(Debug, Clone)]
struct BranchInfo {
    branch_id: String,
    parent_id: String,
    container_id: String,
    image_id: String,
    created_at: Instant,
}

impl DockerForkManager {
    pub fn new(docker: Docker) -> Self {
        Self {
            docker,
            branches: Arc::new(RwLock::new(HashMap::new())),
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a checkpoint of a running container for later forking
    pub async fn checkpoint_container(
        &self,
        container_id: &str,
        checkpoint_id: &str,
    ) -> Result<String> {
        info!(
            "Creating checkpoint of container {} as {}",
            container_id, checkpoint_id
        );

        // Commit the container to create an image with its current state
        let options = CommitContainerOptions {
            container: container_id.to_string(),
            repo: format!("faas-checkpoint-{checkpoint_id}"),
            tag: "latest".to_string(),
            comment: format!("Checkpoint of container {container_id}"),
            ..Default::default()
        };

        let commit_result = self
            .docker
            .commit_container(
                options,
                crate::bollard::container::Config::<String>::default(),
            )
            .await
            .map_err(|e| anyhow!("Failed to commit container: {e}"))?;

        // Use the returned ID or fall back to the image name we specified
        let image_id = commit_result
            .id
            .unwrap_or_else(|| format!("faas-checkpoint-{checkpoint_id}:latest"));

        // Store the checkpoint mapping
        let mut checkpoints = self.checkpoints.write().await;
        checkpoints.insert(checkpoint_id.to_string(), image_id.clone());

        info!(
            "Created checkpoint {} with image {}",
            checkpoint_id, image_id
        );
        Ok(image_id)
    }

    /// Fork a container from a checkpoint, preserving all state
    pub async fn fork_from_checkpoint(&self, checkpoint_id: &str, fork_id: &str) -> Result<String> {
        let start = Instant::now();
        info!(
            "Forking from checkpoint {} to create {}",
            checkpoint_id, fork_id
        );

        // Get the checkpoint image
        let checkpoints = self.checkpoints.read().await;
        let image_id = checkpoints
            .get(checkpoint_id)
            .ok_or_else(|| anyhow!("Checkpoint {checkpoint_id} not found"))?
            .clone();
        drop(checkpoints);

        // Create a new container from the checkpoint image
        let config = CreateContainerOptions {
            name: format!("faas-fork-{fork_id}"),
            platform: None,
        };

        let container_config = crate::bollard::container::Config {
            image: Some(format!("faas-checkpoint-{checkpoint_id}:latest")),
            cmd: Some(vec!["/bin/sh".to_string()]),
            tty: Some(true),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            open_stdin: Some(true),
            stdin_once: Some(false),
            ..Default::default()
        };

        let container = self
            .docker
            .create_container(Some(config), container_config)
            .await
            .map_err(|e| anyhow!("Failed to create forked container: {e}"))?;

        // Start the forked container
        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| anyhow!("Failed to start forked container: {e}"))?;

        // Store branch info
        let branch_info = BranchInfo {
            branch_id: fork_id.to_string(),
            parent_id: checkpoint_id.to_string(),
            container_id: container.id.clone(),
            image_id,
            created_at: Instant::now(),
        };

        let mut branches = self.branches.write().await;
        branches.insert(fork_id.to_string(), branch_info);

        let fork_time = start.elapsed();
        info!("Created fork {} in {:?}", fork_id, fork_time);

        Ok(container.id)
    }

    /// Execute code in a forked container
    pub async fn execute_in_fork(&self, fork_id: &str, command: &str) -> Result<Vec<u8>> {
        let branches = self.branches.read().await;
        let branch_info = branches
            .get(fork_id)
            .ok_or_else(|| anyhow!("Fork {fork_id} not found"))?;
        let container_id = branch_info.container_id.clone();
        drop(branches);

        // Create exec instance
        let exec_config = CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(vec!["/bin/sh", "-c", command]),
            ..Default::default()
        };

        let exec = self
            .docker
            .create_exec(&container_id, exec_config)
            .await
            .map_err(|e| anyhow!("Failed to create exec: {e}"))?;

        // Start exec and collect output
        let start_result = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| anyhow!("Failed to start exec: {e}"))?;

        let output = match start_result {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = Vec::new();
                while let Some(Ok(msg)) = output.next().await {
                    result.extend_from_slice(&msg.into_bytes());
                }
                result
            }
            _ => return Err(anyhow!("Unexpected exec result")),
        };

        Ok(output)
    }

    /// Create a base container with initial state
    pub async fn create_base_container(
        &self,
        base_id: &str,
        image: &str,
        setup_commands: Vec<&str>,
    ) -> Result<String> {
        info!("Creating base container {} from image {}", base_id, image);

        // Create container
        let config = CreateContainerOptions {
            name: format!("faas-base-{base_id}"),
            platform: None,
        };

        let container_config = crate::bollard::container::Config {
            image: Some(image.to_string()),
            cmd: Some(vec!["/bin/sh".to_string()]),
            tty: Some(true),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            open_stdin: Some(true),
            stdin_once: Some(false),
            ..Default::default()
        };

        let container = self
            .docker
            .create_container(Some(config), container_config)
            .await
            .map_err(|e| anyhow!("Failed to create base container: {e}"))?;

        // Start container
        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| anyhow!("Failed to start base container: {e}"))?;

        // Execute setup commands
        for cmd in setup_commands {
            let exec_config = CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(vec!["/bin/sh", "-c", cmd]),
                ..Default::default()
            };

            let exec = self.docker.create_exec(&container.id, exec_config).await?;
            let _result = self.docker.start_exec(&exec.id, None).await?;
        }

        // Create checkpoint of the initialized container
        let checkpoint_id = self.checkpoint_container(&container.id, base_id).await?;

        info!("Base container {} created and checkpointed", base_id);
        Ok(container.id)
    }

    /// Clean up a fork and its resources
    pub async fn cleanup_fork(&self, fork_id: &str) -> Result<()> {
        let mut branches = self.branches.write().await;

        if let Some(branch_info) = branches.remove(fork_id) {
            // Stop and remove container
            let _ = self
                .docker
                .stop_container(&branch_info.container_id, None)
                .await;
            let _ = self
                .docker
                .remove_container(&branch_info.container_id, None)
                .await;

            info!("Cleaned up fork {}", fork_id);
        }

        Ok(())
    }

    /// Get stats about current forks
    pub async fn get_fork_stats(&self) -> ForkStats {
        let branches = self.branches.read().await;
        let checkpoints = self.checkpoints.read().await;

        ForkStats {
            active_forks: branches.len(),
            total_checkpoints: checkpoints.len(),
        }
    }
}

#[derive(Debug)]
pub struct ForkStats {
    pub active_forks: usize,
    pub total_checkpoints: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_real_docker_fork() -> Result<()> {
        // This test requires Docker to be running
        let docker = Docker::connect_with_socket_defaults()?;
        let fork_manager = DockerForkManager::new(docker.clone());

        // Use unique IDs to avoid conflicts
        let test_id = uuid::Uuid::new_v4().to_string();
        let base_id = format!("test-base-{}", &test_id[0..8]);
        let fork_id = format!("test-fork-{}", &test_id[0..8]);

        // Clean up any existing containers first
        let _ = docker
            .remove_container(&format!("faas-base-{}", base_id), None)
            .await;

        // Create base container with state
        let setup_commands = vec![
            "echo 'initial state' > /tmp/state.txt",
            "echo 'data' > /tmp/data.txt",
        ];

        let container_id = fork_manager
            .create_base_container(&base_id, "alpine:latest", setup_commands)
            .await?;

        // Fork from the base
        let forked_container = fork_manager
            .fork_from_checkpoint(&base_id, &fork_id)
            .await?;

        // Verify state is preserved in fork
        let output = fork_manager
            .execute_in_fork(&fork_id, "cat /tmp/state.txt")
            .await?;
        assert!(String::from_utf8_lossy(&output).contains("initial state"));

        let output = fork_manager
            .execute_in_fork(&fork_id, "cat /tmp/data.txt")
            .await?;
        assert!(String::from_utf8_lossy(&output).contains("data"));

        // Cleanup
        fork_manager.cleanup_fork(&fork_id).await?;
        let _ = docker
            .remove_container(&format!("faas-base-{}", base_id), None)
            .await;

        Ok(())
    }
}
