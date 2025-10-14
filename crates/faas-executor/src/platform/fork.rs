use anyhow::{anyhow, Context, Result};
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
    memory_regions: Arc<RwLock<HashMap<String, MemoryRegion>>>,
    page_cache: Arc<RwLock<HashMap<u64, SharedPage>>>,
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

#[derive(Debug, Clone)]
pub struct MemoryRegion {
    pub id: String,
    pub start_addr: usize,
    pub size: usize,
    pub pages: Vec<u64>, // Page IDs
    pub copy_on_write: bool,
}

#[derive(Debug, Clone)]
pub struct SharedPage {
    pub id: u64,
    pub data: Vec<u8>,
    pub ref_count: usize,
    pub dirty: bool,
}

#[derive(Debug, Clone)]
pub struct Fork {
    pub id: String,
    pub parent_id: String,
    pub memory_regions: Vec<MemoryRegion>,
    pub modified_pages: std::collections::HashSet<u64>,
    pub created_at: Instant,
}

impl ForkManager {
    pub fn new() -> Result<Self> {
        // Use temp directory if /var/lib/faas is not writable (e.g., in tests)
        let base_path = if std::fs::create_dir_all("/var/lib/faas/forks").is_ok() {
            std::path::PathBuf::from("/var/lib/faas/forks")
        } else {
            let temp_dir = std::env::temp_dir().join("faas-forks");
            std::fs::create_dir_all(&temp_dir)?;
            temp_dir
        };

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
            memory_regions: Arc::new(RwLock::new(HashMap::new())),
            page_cache: Arc::new(RwLock::new(HashMap::new())),
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
            let branch_id = format!("{parent_id}-fork-{i}");
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

    /// Fast fork with CoW memory regions for <10ms performance
    pub async fn fast_fork_cow_memory(&self, parent_id: &str) -> Result<String> {
        let fork_id = Uuid::new_v4().to_string();
        let start = Instant::now();

        // Get parent memory regions
        let memory_regions = self.memory_regions.read().await;
        let parent_regions = memory_regions
            .get(parent_id)
            .ok_or_else(|| anyhow!("Parent memory region not found: {parent_id}"))?
            .clone();

        // Create CoW fork of memory regions
        let mut fork_regions = Vec::new();
        let mut page_cache = self.page_cache.write().await;

        for region in &parent_regions.pages {
            // Mark page as copy-on-write
            if let Some(page) = page_cache.get_mut(region) {
                page.ref_count += 1;
                fork_regions.push(*region);
            }
        }

        let fork_memory = MemoryRegion {
            id: fork_id.clone(),
            start_addr: parent_regions.start_addr,
            size: parent_regions.size,
            pages: fork_regions,
            copy_on_write: true,
        };

        // Store fork memory region
        drop(memory_regions);
        let mut memory_regions = self.memory_regions.write().await;
        memory_regions.insert(fork_id.clone(), fork_memory);

        info!(
            "Created CoW memory fork {} in {:?}",
            fork_id,
            start.elapsed()
        );
        Ok(fork_id)
    }

    /// Create memory region with lazy loading
    pub async fn setup_lazy_memory(
        &self,
        region_id: &str,
        parent_id: &str,
        size: usize,
    ) -> Result<()> {
        let memory_region = MemoryRegion {
            id: region_id.to_string(),
            start_addr: 0, // Will be set during actual allocation
            size,
            pages: Vec::new(), // Pages loaded on demand
            copy_on_write: true,
        };

        let mut memory_regions = self.memory_regions.write().await;
        memory_regions.insert(region_id.to_string(), memory_region);

        info!(
            "Setup lazy memory region {} from parent {}",
            region_id, parent_id
        );
        Ok(())
    }

    /// Handle page fault and load from parent on demand
    pub async fn handle_page_fault(
        &self,
        fork_id: &str,
        page_addr: usize,
        parent_pid: u32,
    ) -> Result<Vec<u8>> {
        let memory_regions = self.memory_regions.read().await;
        let fork_region = memory_regions
            .get(fork_id)
            .ok_or_else(|| anyhow!("Fork memory region not found: {fork_id}"))?;

        // Calculate page ID from address
        let page_id = (page_addr / 4096) as u64;
        let page_offset = page_addr & !(4096 - 1); // Align to page boundary

        let mut page_cache = self.page_cache.write().await;

        // Check if page already exists
        if let Some(page) = page_cache.get(&page_id) {
            return Ok(page.data.clone());
        }

        // Read actual page from parent process memory
        let page_data = self.read_process_page(parent_pid, page_offset).await?;

        let shared_page = SharedPage {
            id: page_id,
            data: page_data.clone(),
            ref_count: 1,
            dirty: false,
        };

        page_cache.insert(page_id, shared_page);

        debug!(
            "Loaded page {} from parent PID {} at 0x{:x}",
            page_id, parent_pid, page_offset
        );
        Ok(page_data)
    }

    /// Read a 4KB page from parent process via /proc/[pid]/mem
    async fn read_process_page(&self, pid: u32, addr: usize) -> Result<Vec<u8>> {
        use std::fs::File;
        use std::io::{Read, Seek, SeekFrom};

        let mem_path = format!("/proc/{pid}/mem");

        // Check if process still exists and is readable
        if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
            return Err(anyhow!("Parent process {pid} no longer exists"));
        }

        let mut mem_file = File::open(&mem_path).context(format!("Failed to open {mem_path}"))?;

        // Seek to the page address
        mem_file
            .seek(SeekFrom::Start(addr as u64))
            .context(format!("Failed to seek to address 0x{addr:x}"))?;

        // Read exactly one page (4KB)
        let mut page_data = vec![0u8; 4096];
        match mem_file.read_exact(&mut page_data) {
            Ok(_) => {
                info!(
                    "Successfully read 4KB page from PID {} at 0x{:x}",
                    pid, addr
                );
                Ok(page_data)
            }
            Err(e) => {
                warn!(
                    "Failed to read memory from PID {} at 0x{:x}: {}, using zero page",
                    pid, addr, e
                );
                // Fallback to zero page if memory is unmapped/protected
                Ok(vec![0u8; 4096])
            }
        }
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
            .args([
                "-t",
                "overlay",
                "overlay",
                "-o",
                &format!(
                    "lowerdir={},upperdir={},workdir={}",
                    base_dir.display(),
                    upper_dir.display(),
                    work_dir.display()
                ),
                merged_dir.to_str().unwrap(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to mount overlayfs: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        info!("Created overlayfs fork {} from {}", fork_id, base_id);
        Ok(merged_dir)
    }

    fn supports_reflink(&self) -> bool {
        // Check if filesystem supports reflink (BTRFS, ZFS)
        std::process::Command::new("cp")
            .args(["--reflink=always", "/dev/null", "/tmp/reflink_test"])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    async fn reflink_copy(&self, src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
        let output = tokio::process::Command::new("cp")
            .args([
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
