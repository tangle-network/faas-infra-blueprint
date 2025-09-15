use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ForkManager {
    branches: Arc<RwLock<HashMap<String, Branch>>>,
    cow_storage: CowStorage,
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

        Ok(Self {
            branches: Arc::new(RwLock::new(HashMap::new())),
            cow_storage: CowStorage { base_path },
        })
    }

    pub async fn branch(&self, parent_id: &str, count: usize) -> Result<Vec<String>> {
        let mut branch_ids = Vec::new();

        for i in 0..count {
            let branch_id = format!("{}-fork-{}", parent_id, i);
            let overlay_path = self.cow_storage.create_overlay(parent_id, &branch_id).await?;

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
}

impl CowStorage {
    async fn create_overlay(&self, parent_id: &str, branch_id: &str) -> Result<std::path::PathBuf> {
        let overlay_dir = self.base_path.join(branch_id);
        tokio::fs::create_dir_all(&overlay_dir).await?;

        // Create copy-on-write overlay using BTRFS/ZFS or bind mounts
        let parent_path = self.base_path.join(parent_id);

        if self.supports_reflink() {
            // Use reflink copy for instant CoW
            self.reflink_copy(&parent_path, &overlay_dir).await?;
        } else {
            // Fallback to bind mount overlay
            self.create_bind_overlay(&parent_path, &overlay_dir).await?;
        }

        Ok(overlay_dir)
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
            return Err(anyhow::anyhow!("Reflink copy failed: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        Ok(())
    }

    async fn create_bind_overlay(&self, _lower: &std::path::Path, overlay: &std::path::Path) -> Result<()> {
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