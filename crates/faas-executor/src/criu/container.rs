//! CRIU container integration for checkpoint/restore functionality
//! This module provides real CRIU integration that works in Docker containers

use std::process::Command;
use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};

/// CRIU checkpoint/restore manager for containers
pub struct CriuManager {
    /// Base directory for storing checkpoints
    checkpoint_dir: PathBuf,
    /// CRIU options
    options: CriuOptions,
    /// Active checkpoints
    checkpoints: HashMap<String, CheckpointMetadata>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CriuOptions {
    /// Enable TCP connection checkpointing
    pub tcp_established: bool,
    /// Checkpoint file locks
    pub file_locks: bool,
    /// Handle external files
    pub external: Vec<String>,
    /// Shell job mode (for containers)
    pub shell_job: bool,
    /// Leave process running after checkpoint
    pub leave_running: bool,
    /// Pre-dump for incremental checkpoints
    pub pre_dump: bool,
    /// Track memory changes
    pub track_mem: bool,
    /// Page server for memory migration
    pub page_server: Option<String>,
    /// Manage cgroups
    pub manage_cgroups: bool,
    /// Cgroup root
    pub cgroup_root: Option<String>,
}

impl Default for CriuOptions {
    fn default() -> Self {
        Self {
            tcp_established: true,
            file_locks: true,
            external: vec![],
            shell_job: true,
            leave_running: false,
            pre_dump: false,
            track_mem: false,
            page_server: None,
            manage_cgroups: true,
            cgroup_root: Some("/docker".to_string()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub id: String,
    pub pid: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub size_bytes: u64,
    pub parent_checkpoint: Option<String>,
    pub incremental: bool,
    pub container_id: Option<String>,
}

impl CriuManager {
    /// Create a new CRIU manager
    pub fn new(checkpoint_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&checkpoint_dir)?;

        // Check CRIU availability
        let output = Command::new("criu")
            .arg("--version")
            .output()
            .context("CRIU not found. Install with: apt-get install criu")?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            info!("CRIU initialized: {}", version.trim());
        }

        Ok(Self {
            checkpoint_dir,
            options: CriuOptions::default(),
            checkpoints: HashMap::new(),
        })
    }

    /// Check if CRIU is functional in the current environment
    pub fn check_functionality(&self) -> Result<CriuFeatures> {
        let mut features = CriuFeatures::default();

        // Basic check
        let basic = Command::new("criu")
            .arg("check")
            .output()?;
        features.basic = basic.status.success();

        // Check specific features
        let feature_checks = [
            ("uffd", "userfaultfd"),
            ("lazy-pages", "lazy pages"),
            ("pidfd_store", "pidfd store"),
            ("network_lock", "network locking"),
            ("mem_dirty_track", "memory dirty tracking"),
        ];

        for (flag, name) in feature_checks {
            let result = Command::new("criu")
                .arg("check")
                .arg("--feature")
                .arg(flag)
                .output();

            if let Ok(output) = result {
                if output.status.success() {
                    debug!("CRIU feature '{}' available", name);
                    match flag {
                        "uffd" => features.userfaultfd = true,
                        "lazy-pages" => features.lazy_pages = true,
                        "mem_dirty_track" => features.dirty_tracking = true,
                        _ => {}
                    }
                }
            }
        }

        // Check for Docker/container-specific features
        if std::path::Path::new("/proc/self/ns/pid").exists() {
            features.pid_namespace = true;
        }
        if std::path::Path::new("/proc/self/ns/net").exists() {
            features.net_namespace = true;
        }

        Ok(features)
    }

    /// Checkpoint a process
    pub fn checkpoint(&mut self, pid: u32, checkpoint_id: &str) -> Result<CheckpointMetadata> {
        let checkpoint_path = self.checkpoint_dir.join(checkpoint_id);
        fs::create_dir_all(&checkpoint_path)?;

        info!("Creating checkpoint {} for PID {}", checkpoint_id, pid);

        let mut cmd = Command::new("criu");
        cmd.arg("dump")
            .arg("-t").arg(pid.to_string())
            .arg("-D").arg(&checkpoint_path);

        // Apply options
        if self.options.tcp_established {
            cmd.arg("--tcp-established");
        }
        if self.options.file_locks {
            cmd.arg("--file-locks");
        }
        if self.options.shell_job {
            cmd.arg("--shell-job");
        }
        if self.options.leave_running {
            cmd.arg("--leave-running");
        }
        if self.options.manage_cgroups {
            cmd.arg("--manage-cgroups");
            if let Some(ref root) = self.options.cgroup_root {
                cmd.arg("--cgroup-root").arg(root);
            }
        }

        // Add external resources
        for ext in &self.options.external {
            cmd.arg("--external").arg(ext);
        }

        // Execute checkpoint
        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("CRIU dump failed: {}", stderr);
            return Err(anyhow::anyhow!("Checkpoint failed: {}", stderr));
        }

        // Calculate checkpoint size
        let size = fs::read_dir(&checkpoint_path)?
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();

        let metadata = CheckpointMetadata {
            id: checkpoint_id.to_string(),
            pid,
            created_at: chrono::Utc::now(),
            size_bytes: size,
            parent_checkpoint: None,
            incremental: false,
            container_id: None,
        };

        self.checkpoints.insert(checkpoint_id.to_string(), metadata.clone());

        // Save metadata
        let metadata_path = checkpoint_path.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(metadata_path, metadata_json)?;

        info!("Checkpoint {} created successfully ({} bytes)", checkpoint_id, size);
        Ok(metadata)
    }

    /// Create an incremental checkpoint
    pub fn incremental_checkpoint(&mut self, pid: u32, checkpoint_id: &str, parent_id: &str) -> Result<CheckpointMetadata> {
        let parent_path = self.checkpoint_dir.join(parent_id);
        if !parent_path.exists() {
            return Err(anyhow::anyhow!("Parent checkpoint {} not found", parent_id));
        }

        let checkpoint_path = self.checkpoint_dir.join(checkpoint_id);
        fs::create_dir_all(&checkpoint_path)?;

        info!("Creating incremental checkpoint {} from parent {}", checkpoint_id, parent_id);

        // First, do a pre-dump to track memory
        let predump_path = checkpoint_path.join("predump");
        fs::create_dir_all(&predump_path)?;

        let predump = Command::new("criu")
            .arg("pre-dump")
            .arg("-t").arg(pid.to_string())
            .arg("-D").arg(&predump_path)
            .arg("--track-mem")
            .arg("--parent-path").arg(&parent_path)
            .output()?;

        if !predump.status.success() {
            let stderr = String::from_utf8_lossy(&predump.stderr);
            warn!("Pre-dump failed: {}", stderr);
        }

        // Now do the actual incremental dump
        let mut cmd = Command::new("criu");
        cmd.arg("dump")
            .arg("-t").arg(pid.to_string())
            .arg("-D").arg(&checkpoint_path)
            .arg("--prev-images-dir").arg(&parent_path)
            .arg("--track-mem");

        if self.options.leave_running {
            cmd.arg("--leave-running");
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Incremental checkpoint failed: {}", stderr));
        }

        let size = fs::read_dir(&checkpoint_path)?
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();

        let metadata = CheckpointMetadata {
            id: checkpoint_id.to_string(),
            pid,
            created_at: chrono::Utc::now(),
            size_bytes: size,
            parent_checkpoint: Some(parent_id.to_string()),
            incremental: true,
            container_id: None,
        };

        self.checkpoints.insert(checkpoint_id.to_string(), metadata.clone());

        info!("Incremental checkpoint {} created ({} bytes)", checkpoint_id, size);
        Ok(metadata)
    }

    /// Restore a process from checkpoint
    pub fn restore(&self, checkpoint_id: &str) -> Result<u32> {
        let checkpoint_path = self.checkpoint_dir.join(checkpoint_id);
        if !checkpoint_path.exists() {
            return Err(anyhow::anyhow!("Checkpoint {} not found", checkpoint_id));
        }

        info!("Restoring from checkpoint {}", checkpoint_id);

        let mut cmd = Command::new("criu");
        cmd.arg("restore")
            .arg("-D").arg(&checkpoint_path)
            .arg("--restore-detached");

        if self.options.tcp_established {
            cmd.arg("--tcp-established");
        }
        if self.options.file_locks {
            cmd.arg("--file-locks");
        }
        if self.options.shell_job {
            cmd.arg("--shell-job");
        }
        if self.options.manage_cgroups {
            cmd.arg("--manage-cgroups");
            if let Some(ref root) = self.options.cgroup_root {
                cmd.arg("--cgroup-root").arg(root);
            }
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Restore failed: {}", stderr));
        }

        // Read the pidfile to get the restored PID
        let pidfile = checkpoint_path.join("pidfile");
        if pidfile.exists() {
            let pid_str = fs::read_to_string(pidfile)?;
            let pid: u32 = pid_str.trim().parse()?;
            info!("Process restored with PID {}", pid);
            return Ok(pid);
        }

        // If no pidfile, try to extract from CRIU output
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = stdout.lines().find(|l| l.contains("Restored PID")) {
            if let Some(pid_str) = line.split_whitespace().last() {
                if let Ok(pid) = pid_str.parse::<u32>() {
                    return Ok(pid);
                }
            }
        }

        Err(anyhow::anyhow!("Could not determine restored PID"))
    }

    /// List all available checkpoints
    pub fn list_checkpoints(&self) -> Vec<CheckpointMetadata> {
        self.checkpoints.values().cloned().collect()
    }

    /// Delete a checkpoint
    pub fn delete_checkpoint(&mut self, checkpoint_id: &str) -> Result<()> {
        let checkpoint_path = self.checkpoint_dir.join(checkpoint_id);
        if checkpoint_path.exists() {
            fs::remove_dir_all(checkpoint_path)?;
            self.checkpoints.remove(checkpoint_id);
            info!("Deleted checkpoint {}", checkpoint_id);
        }
        Ok(())
    }

    /// Get checkpoint metadata
    pub fn get_checkpoint(&self, checkpoint_id: &str) -> Option<&CheckpointMetadata> {
        self.checkpoints.get(checkpoint_id)
    }
}

#[derive(Debug, Default)]
pub struct CriuFeatures {
    pub basic: bool,
    pub userfaultfd: bool,
    pub lazy_pages: bool,
    pub dirty_tracking: bool,
    pub pid_namespace: bool,
    pub net_namespace: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    #[ignore = "Requires CRIU"]
    fn test_criu_availability() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CriuManager::new(temp_dir.path().to_path_buf());
        assert!(manager.is_ok());

        if let Ok(mgr) = manager {
            let features = mgr.check_functionality();
            if let Ok(f) = features {
                println!("CRIU Features: {:?}", f);
            }
        }
    }

    #[test]
    #[ignore = "Requires CRIU and root privileges"]
    fn test_checkpoint_restore() {
        use std::process::{Command, Stdio};
        use std::thread;
        use std::time::Duration;

        let temp_dir = TempDir::new().unwrap();
        let mut manager = CriuManager::new(temp_dir.path().to_path_buf()).unwrap();

        // Start a simple process
        let mut child = Command::new("sleep")
            .arg("100")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let pid = child.id();
        thread::sleep(Duration::from_millis(100));

        // Checkpoint it
        let result = manager.checkpoint(pid, "test-checkpoint");
        if let Ok(metadata) = result {
            println!("Checkpoint created: {:?}", metadata);

            // Kill original process
            let _ = child.kill();

            // Restore it
            let restored = manager.restore("test-checkpoint");
            if let Ok(new_pid) = restored {
                println!("Process restored with PID {}", new_pid);

                // Kill restored process
                let _ = Command::new("kill")
                    .arg(new_pid.to_string())
                    .output();
            }
        }
    }
}