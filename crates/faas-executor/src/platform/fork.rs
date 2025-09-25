use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

pub struct ForkManager {
    branches: Arc<RwLock<HashMap<String, Branch>>>,
    cow_storage: CowStorage,
    overlayfs_available: bool,
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub id: String,
    pub parent: String,
    pub overlay_path: std::path::PathBuf,
    pub created_at: std::time::Instant,
}

struct CowStorage {
    base_path: std::path::PathBuf,
}

impl ForkManager {
    pub fn new() -> Result<Self> {
        let base_path = std::path::PathBuf::from("/var/lib/faas/forks");
        std::fs::create_dir_all(&base_path)?;

        // Check if overlayfs is available
        let overlayfs_available = Self::check_overlayfs_support();
        if overlayfs_available {
            info!("OverlayFS support detected for fast forking");
        } else {
            warn!("OverlayFS not available, using fallback CoW method");
        }

        Ok(Self {
            branches: Arc::new(RwLock::new(HashMap::new())),
            cow_storage: CowStorage { base_path },
            overlayfs_available,
        })
    }

    fn check_overlayfs_support() -> bool {
        // Check if overlayfs is available in /proc/filesystems
        if let Ok(filesystems) = std::fs::read_to_string("/proc/filesystems") {
            filesystems.contains("overlay")
        } else {
            false
        }
    }

    pub async fn branch(&self, parent_id: &str, count: usize) -> Result<Vec<String>> {
        let mut branch_ids = Vec::new();

        for i in 0..count {
            let branch_id = format!("{}-fork-{}", parent_id, i);
            let overlay_path = self
                .cow_storage
                .create_overlay(parent_id, &branch_id)
                .await?;

            let branch = Branch {
                id: branch_id.clone(),
                parent: parent_id.to_string(),
                overlay_path,
                created_at: std::time::Instant::now(),
            };

            let mut branches = self.branches.write().await;
            branches.insert(branch_id.clone(), branch);
            branch_ids.push(branch_id);
        }

        Ok(branch_ids)
    }

    pub async fn get_branch(&self, id: &str) -> Option<Branch> {
        let branches = self.branches.read().await;
        branches.get(id).cloned()
    }

    /// Fast fork using overlayfs for near-zero overhead
    pub async fn fast_fork(&self, base: &str) -> Result<String> {
        let fork_id = Uuid::new_v4().to_string();
        let start = Instant::now();

        let overlay_path = if self.overlayfs_available {
            self.cow_storage.create_overlayfs(base, &fork_id).await?
        } else {
            self.cow_storage.create_overlay(base, &fork_id).await?
        };

        let branch = Branch {
            id: fork_id.clone(),
            parent: base.to_string(),
            overlay_path,
            created_at: Instant::now(),
        };

        let mut branches = self.branches.write().await;
        branches.insert(fork_id.clone(), branch);

        debug!("Created fast fork {} in {:?}", fork_id, start.elapsed());
        Ok(fork_id)
    }

    /// Clean up a fork and reclaim resources
    pub async fn cleanup_fork(&self, fork_id: &str) -> Result<()> {
        let mut branches = self.branches.write().await;

        if let Some(branch) = branches.remove(fork_id) {
            // Unmount if using overlayfs
            if self.overlayfs_available {
                let _ = Command::new("umount")
                    .arg(&branch.overlay_path)
                    .output()
                    .await;
            }

            // Remove directory
            tokio::fs::remove_dir_all(branch.overlay_path.parent().unwrap()).await?;
            info!("Cleaned up fork {}", fork_id);
        }

        Ok(())
    }
}

impl CowStorage {
    async fn create_overlay(&self, parent_id: &str, branch_id: &str) -> Result<std::path::PathBuf> {
        let overlay_dir = self.base_path.join(branch_id);
        tokio::fs::create_dir_all(&overlay_dir).await?;

        // Try overlayfs first for best performance
        if self.can_use_overlayfs() {
            self.create_overlayfs(parent_id, branch_id).await
        } else if self.supports_reflink() {
            // Use reflink copy for instant CoW
            let parent_path = self.base_path.join(parent_id);
            self.reflink_copy(&parent_path, &overlay_dir).await?;
            Ok(overlay_dir)
        } else {
            // Fallback to bind mount overlay
            let parent_path = self.base_path.join(parent_id);
            self.create_bind_overlay(&parent_path, &overlay_dir).await?;
            Ok(overlay_dir)
        }
    }

    fn can_use_overlayfs(&self) -> bool {
        // Check if we can use overlayfs (requires Linux and appropriate permissions)
        #[cfg(target_os = "linux")]
        {
            if let Ok(filesystems) = std::fs::read_to_string("/proc/filesystems") {
                return filesystems.contains("overlay");
            }
        }
        false
    }

    /// Create fast overlayfs-based fork
    async fn create_overlayfs(&self, base_id: &str, fork_id: &str) -> Result<PathBuf> {
        let fork_dir = self.base_path.join(fork_id);

        // Create overlay structure
        let upper_dir = fork_dir.join("upper");
        let work_dir = fork_dir.join("work");
        let merged_dir = fork_dir.join("merged");

        tokio::fs::create_dir_all(&upper_dir).await?;
        tokio::fs::create_dir_all(&work_dir).await?;
        tokio::fs::create_dir_all(&merged_dir).await?;

        // Mount overlay
        let base_dir = self.base_path.join(base_id);

        let output = Command::new("mount")
            .args(&[
                "-t", "overlay", "overlay",
                "-o", &format!(
                    "lowerdir={},upperdir={},workdir={}",
                    base_dir.display(),
                    upper_dir.display(),
                    work_dir.display()
                ),
                merged_dir.to_str().unwrap()
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!("Failed to mount overlayfs: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        info!("Created overlayfs fork {} from {}", fork_id, base_id);
        Ok(merged_dir)
    }

    fn supports_reflink(&self) -> bool {
        // Check if filesystem supports reflink (BTRFS, ZFS)
        std::process::Command::new("cp")
            .args(&["--reflink=always", "/dev/null", "/tmp/reflink_test"])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    async fn reflink_copy(&self, src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
        let output = tokio::process::Command::new("cp")
            .args(&[
                "--reflink=always",
                "-r",
                src.to_str().unwrap(),
                dst.to_str().unwrap(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Reflink copy failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    async fn create_bind_overlay(
        &self,
        _lower: &std::path::Path,
        overlay: &std::path::Path,
    ) -> Result<()> {
        // Create overlay filesystem structure
        let work_dir = overlay.join("work");
        let upper_dir = overlay.join("upper");
        let merged_dir = overlay.join("merged");

        for dir in [&work_dir, &upper_dir, &merged_dir] {
            tokio::fs::create_dir_all(dir).await?;
        }

        Ok(())
    }
}
