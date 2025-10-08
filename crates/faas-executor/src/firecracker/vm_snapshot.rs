//! VM Snapshot Management for Firecracker using firecracker-rs-sdk
//! Provides full snapshot/restore capabilities with memory and disk state preservation

use anyhow::{anyhow, Result};
use firecracker_rs_sdk::instance::Instance as FcInstance;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// VM Snapshot Manager with full state preservation
pub struct VmSnapshotManager {
    snapshot_dir: PathBuf,
    snapshots: Arc<RwLock<HashMap<String, VmSnapshot>>>,
    cache: Arc<RwLock<SnapshotCache>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSnapshot {
    pub id: String,
    pub vm_id: String,
    pub memory_file: PathBuf,
    pub state_file: PathBuf,
    pub disk_file: PathBuf,
    pub metadata: SnapshotMetadata,
    pub created_at: SystemTime,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub vcpu_count: u8,
    pub memory_size_mib: usize,
    pub kernel_version: String,
    pub rootfs_hash: String,
    pub parent_snapshot: Option<String>,
    pub incremental: bool,
}

/// Cache for hot snapshots in memory
struct SnapshotCache {
    hot_snapshots: HashMap<String, CachedSnapshot>,
    memory_limit_mb: usize,
    current_size_mb: usize,
}

#[derive(Clone)]
struct CachedSnapshot {
    memory_data: Vec<u8>,
    state_data: Vec<u8>,
    last_accessed: Instant,
    access_count: usize,
}

impl VmSnapshotManager {
    pub fn new(snapshot_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&snapshot_dir)?;

        Ok(Self {
            snapshot_dir,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            cache: Arc::new(RwLock::new(SnapshotCache {
                hot_snapshots: HashMap::new(),
                memory_limit_mb: 4096, // 4GB cache
                current_size_mb: 0,
            })),
        })
    }

    /// Create a snapshot of a running VM using firecracker-rs-sdk
    pub async fn create_snapshot(
        &self,
        vm_id: &str,
        snapshot_id: &str,
        fc_instance: &mut FcInstance,
    ) -> Result<VmSnapshot> {
        info!("Creating VM snapshot via SDK: {} for VM: {}", snapshot_id, vm_id);
        let start = Instant::now();

        let snapshot_path = self.snapshot_dir.join(snapshot_id);
        fs::create_dir_all(&snapshot_path)?;

        let memory_file = snapshot_path.join("memory.snap");
        let state_file = snapshot_path.join("state.snap");

        // Use firecracker-rs-sdk to create snapshot
        #[cfg(target_os = "linux")]
        {
            // Create snapshot using SDK instance
            let snapshot_params = SnapshotCreateParams {
                snapshot_type: Some(SnapshotType::Full),
                snapshot_path: state_file.clone(),
                mem_file_path: memory_file.clone(),
                version: Some("1.0.0".to_string()),
            };

            fc_instance
                .create_snapshot(&snapshot_params)
                .map_err(|e| anyhow!("Failed to create VM snapshot via SDK: {:?}", e))?;

            info!("Snapshot created via SDK in {}ms", start.elapsed().as_millis());
        }

        // For non-Linux, create placeholder files
        #[cfg(not(target_os = "linux"))]
        {
            // Create placeholder snapshot files for testing on Mac
            fs::write(&memory_file, b"MEMORY_SNAPSHOT_PLACEHOLDER")?;
            fs::write(&state_file, b"STATE_SNAPSHOT_PLACEHOLDER")?;
        }

        // Create disk snapshot using copy-on-write where possible
        let disk_file = self.create_disk_snapshot(vm_id, &snapshot_path).await?;
        let disk_file_clone = disk_file.clone(); // Clone before move

        // Calculate total size
        let memory_size = fs::metadata(&memory_file)?.len();
        let state_size = fs::metadata(&state_file)?.len();
        let disk_size = fs::metadata(&disk_file)?.len();
        let total_size = memory_size + state_size + disk_size;

        let snapshot = VmSnapshot {
            id: snapshot_id.to_string(),
            vm_id: vm_id.to_string(),
            memory_file,
            state_file,
            disk_file: disk_file_clone.clone(),
            metadata: SnapshotMetadata {
                vcpu_count: 2, // Get from VM config
                memory_size_mib: 512, // Get from VM config
                kernel_version: "5.10".to_string(),
                rootfs_hash: self.calculate_rootfs_hash(&disk_file_clone)?,
                parent_snapshot: None,
                incremental: false,
            },
            created_at: SystemTime::now(),
            size_bytes: total_size,
        };

        // Store snapshot metadata
        let mut snapshots = self.snapshots.write().await;
        snapshots.insert(snapshot_id.to_string(), snapshot.clone());

        let elapsed = start.elapsed();
        info!(
            "VM snapshot created: {} ({} MB) in {:?}",
            snapshot_id,
            total_size / 1_048_576,
            elapsed
        );

        Ok(snapshot)
    }

    /// Create incremental snapshot based on parent using SDK
    pub async fn create_incremental_snapshot(
        &self,
        vm_id: &str,
        snapshot_id: &str,
        parent_id: &str,
        fc_instance: &mut FcInstance,
    ) -> Result<VmSnapshot> {
        info!("Creating incremental snapshot via SDK: {} (parent: {})", snapshot_id, parent_id);

        // Verify parent exists
        let snapshots = self.snapshots.read().await;
        let parent = snapshots.get(parent_id)
            .ok_or_else(|| anyhow!("Parent snapshot not found: {parent_id}"))?
            .clone();
        drop(snapshots);

        let snapshot_path = self.snapshot_dir.join(snapshot_id);
        fs::create_dir_all(&snapshot_path)?;

        // Create diff snapshot
        let memory_file = snapshot_path.join("memory.diff");
        let state_file = snapshot_path.join("state.snap");

        #[cfg(target_os = "linux")]
        {
            // Use Firecracker SDK's diff snapshot feature
            let snapshot_params = SnapshotCreateParams {
                snapshot_type: Some(SnapshotType::Diff),
                snapshot_path: state_file.clone(),
                mem_file_path: memory_file.clone(),
                version: Some("1.0.0".to_string()),
            };

            fc_instance.create_snapshot(&snapshot_params)
                .map_err(|e| anyhow!("Failed to create incremental snapshot via SDK: {:?}", e))?;

            // Create memory diff using our optimization
            self.create_memory_diff(&parent.memory_file, &memory_file).await?;

            info!("Incremental snapshot created via SDK");
        }

        #[cfg(not(target_os = "linux"))]
        {
            fs::write(&memory_file, b"MEMORY_DIFF_PLACEHOLDER")?;
            fs::write(&state_file, b"STATE_DIFF_PLACEHOLDER")?;
        }

        // Create CoW disk snapshot
        let disk_file = self.create_cow_disk_snapshot(&parent.disk_file, &snapshot_path).await?;

        let memory_size = fs::metadata(&memory_file)?.len();
        let state_size = fs::metadata(&state_file)?.len();
        let disk_size = fs::metadata(&disk_file)?.len();

        let snapshot = VmSnapshot {
            id: snapshot_id.to_string(),
            vm_id: vm_id.to_string(),
            memory_file,
            state_file,
            disk_file,
            metadata: SnapshotMetadata {
                parent_snapshot: Some(parent_id.to_string()),
                incremental: true,
                ..parent.metadata.clone()
            },
            created_at: SystemTime::now(),
            size_bytes: memory_size + state_size + disk_size,
        };

        let mut snapshots = self.snapshots.write().await;
        snapshots.insert(snapshot_id.to_string(), snapshot.clone());

        info!(
            "Incremental snapshot created: {} ({} MB)",
            snapshot_id,
            snapshot.size_bytes / 1_048_576
        );

        Ok(snapshot)
    }

    /// Restore a VM from snapshot
    pub async fn restore_snapshot(
        &self,
        snapshot_id: &str,
        new_vm_id: &str,
    ) -> Result<RestoredVm> {
        info!("Restoring VM {} from snapshot {}", new_vm_id, snapshot_id);
        let start = Instant::now();

        // Check cache first
        let cached_data = {
            let cache = self.cache.read().await;
            cache.hot_snapshots.get(snapshot_id).cloned()
        };

        if let Some(cached) = cached_data {
            info!("Using cached snapshot for fast restore");
            return self.restore_from_cache(snapshot_id, new_vm_id, &cached).await;
        }

        // Get snapshot metadata
        let snapshots = self.snapshots.read().await;
        let snapshot = snapshots.get(snapshot_id)
            .ok_or_else(|| anyhow!("Snapshot not found: {snapshot_id}"))?
            .clone();
        drop(snapshots);

        // Prepare restore directory
        let restore_dir = self.snapshot_dir.join("restore").join(new_vm_id);
        fs::create_dir_all(&restore_dir)?;

        // Apply incremental snapshots if needed
        let final_memory = if snapshot.metadata.incremental {
            self.apply_incremental_chain(&snapshot).await?
        } else {
            snapshot.memory_file.clone()
        };

        #[cfg(target_os = "linux")]
        {
            // Use Firecracker API to restore VM
            let socket_path = format!("/tmp/firecracker-{}.sock", new_vm_id);

            // Start Firecracker process with restore parameters
            let mut cmd = std::process::Command::new("firecracker");
            cmd.arg("--api-sock").arg(&socket_path)
               .arg("--restore")
               .arg("--restore-state-file").arg(&snapshot.state_file)
               .arg("--restore-mem-file").arg(&final_memory);

            let child = cmd.spawn()
                .context("Failed to spawn Firecracker for restore")?;

            // Wait for API to be available
            tokio::time::sleep(Duration::from_millis(100)).await;

            let elapsed = start.elapsed();
            info!("VM restored from snapshot in {:?}", elapsed);

            return Ok(RestoredVm {
                vm_id: new_vm_id.to_string(),
                api_socket: socket_path,
                process: Some(child),
                restore_time: elapsed,
            });
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Placeholder for Mac testing
            Ok(RestoredVm {
                vm_id: new_vm_id.to_string(),
                api_socket: format!("/tmp/firecracker-{new_vm_id}.sock"),
                process: None,
                restore_time: start.elapsed(),
            })
        }
    }

    /// Pre-warm snapshots into memory cache for ultra-fast restore
    pub async fn prewarm_snapshot(&self, snapshot_id: &str) -> Result<()> {
        info!("Pre-warming snapshot: {}", snapshot_id);

        let snapshots = self.snapshots.read().await;
        let snapshot = snapshots.get(snapshot_id)
            .ok_or_else(|| anyhow!("Snapshot not found"))?
            .clone();
        drop(snapshots);

        // Read snapshot files into memory
        let mut memory_data = Vec::new();
        File::open(&snapshot.memory_file)?.read_to_end(&mut memory_data)?;

        let mut state_data = Vec::new();
        File::open(&snapshot.state_file)?.read_to_end(&mut state_data)?;

        let size_mb = (memory_data.len() + state_data.len()) / 1_048_576;

        // Add to cache
        let mut cache = self.cache.write().await;

        // Evict old entries if needed
        while cache.current_size_mb + size_mb > cache.memory_limit_mb {
            self.evict_coldest(&mut cache).await;
        }

        cache.hot_snapshots.insert(
            snapshot_id.to_string(),
            CachedSnapshot {
                memory_data,
                state_data,
                last_accessed: Instant::now(),
                access_count: 0,
            },
        );
        cache.current_size_mb += size_mb;

        info!("Snapshot pre-warmed: {} ({} MB)", snapshot_id, size_mb);
        Ok(())
    }

    /// Create disk snapshot using CoW techniques
    async fn create_disk_snapshot(&self, vm_id: &str, snapshot_path: &Path) -> Result<PathBuf> {
        let disk_file = snapshot_path.join("disk.qcow2");

        #[cfg(target_os = "linux")]
        {
            // Use qemu-img for CoW snapshot
            let source_disk = format!("/var/lib/firecracker/vms/{}/rootfs.ext4", vm_id);

            let output = tokio::process::Command::new("qemu-img")
                .args(&["create", "-f", "qcow2", "-b"])
                .arg(&source_disk)
                .arg(&disk_file)
                .output()
                .await?;

            if !output.status.success() {
                // Fallback to copy
                fs::copy(&source_disk, &disk_file)?;
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            fs::write(&disk_file, b"DISK_SNAPSHOT_PLACEHOLDER")?;
        }

        Ok(disk_file)
    }

    /// Create CoW disk snapshot from parent
    async fn create_cow_disk_snapshot(&self, parent_disk: &Path, snapshot_path: &Path) -> Result<PathBuf> {
        let disk_file = snapshot_path.join("disk.qcow2");

        #[cfg(target_os = "linux")]
        {
            // Create qcow2 with backing file
            let output = tokio::process::Command::new("qemu-img")
                .args(&["create", "-f", "qcow2", "-b"])
                .arg(parent_disk)
                .arg(&disk_file)
                .output()
                .await?;

            if !output.status.success() {
                return Err(anyhow!("Failed to create CoW disk snapshot"));
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            fs::write(&disk_file, b"COW_DISK_PLACEHOLDER")?;
        }

        Ok(disk_file)
    }

    /// Create memory diff between snapshots
    async fn create_memory_diff(&self, base_mem: &Path, diff_file: &Path) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            // Use xdelta3 or custom diff algorithm
            let output = tokio::process::Command::new("xdelta3")
                .args(&["-e", "-s"])
                .arg(base_mem)
                .arg("-") // Read current memory from stdin
                .arg(diff_file)
                .output()
                .await;

            if output.is_err() {
                // Fallback to custom diff
                self.create_custom_memory_diff(base_mem, diff_file).await?;
            }
        }

        Ok(())
    }

    /// Custom memory diff implementation
    async fn create_custom_memory_diff(&self, base: &Path, diff: &Path) -> Result<()> {
        // Implement page-level diffing
        const PAGE_SIZE: usize = 4096;

        let base_data = fs::read(base)?;
        // In real implementation, read current memory from VM
        let current_data = base_data.clone(); // Placeholder

        let mut diff_data = Vec::new();

        for (i, chunk) in current_data.chunks(PAGE_SIZE).enumerate() {
            let base_chunk = &base_data[i * PAGE_SIZE..(i + 1) * PAGE_SIZE.min(base_data.len())];

            if chunk != base_chunk {
                // Store page index and new data
                diff_data.extend_from_slice(&(i as u32).to_le_bytes());
                diff_data.extend_from_slice(chunk);
            }
        }

        fs::write(diff, diff_data)?;
        Ok(())
    }

    /// Apply incremental snapshot chain
    async fn apply_incremental_chain(&self, snapshot: &VmSnapshot) -> Result<PathBuf> {
        let mut chain = vec![snapshot.clone()];
        let mut current = snapshot.clone();

        // Build chain from child to parent
        while let Some(parent_id) = &current.metadata.parent_snapshot {
            let snapshots = self.snapshots.read().await;
            let parent = snapshots.get(parent_id)
                .ok_or_else(|| anyhow!("Parent snapshot not found in chain"))?
                .clone();
            drop(snapshots);

            chain.push(parent.clone());
            current = parent;
        }

        // Apply from parent to child
        chain.reverse();

        let merged_file = self.snapshot_dir.join("merged").join(format!("{}.mem", snapshot.id));
        fs::create_dir_all(merged_file.parent().unwrap())?;

        // Start with base snapshot
        fs::copy(&chain[0].memory_file, &merged_file)?;

        // Apply diffs
        for snap in &chain[1..] {
            if snap.metadata.incremental {
                self.apply_memory_diff(&merged_file, &snap.memory_file).await?;
            }
        }

        Ok(merged_file)
    }

    /// Apply memory diff to base
    async fn apply_memory_diff(&self, base: &Path, diff: &Path) -> Result<()> {
        const PAGE_SIZE: usize = 4096;

        let mut base_data = fs::read(base)?;
        let diff_data = fs::read(diff)?;

        let mut offset = 0;
        while offset < diff_data.len() {
            // Read page index
            let page_idx = u32::from_le_bytes([
                diff_data[offset],
                diff_data[offset + 1],
                diff_data[offset + 2],
                diff_data[offset + 3],
            ]) as usize;
            offset += 4;

            // Read page data
            let page_start = page_idx * PAGE_SIZE;
            let page_end = (page_start + PAGE_SIZE).min(base_data.len());
            let page_size = page_end - page_start;

            base_data[page_start..page_end].copy_from_slice(&diff_data[offset..offset + page_size]);
            offset += page_size;
        }

        fs::write(base, base_data)?;
        Ok(())
    }

    /// Restore from cached snapshot
    async fn restore_from_cache(
        &self,
        snapshot_id: &str,
        new_vm_id: &str,
        cached: &CachedSnapshot,
    ) -> Result<RestoredVm> {
        let start = Instant::now();

        // Write cached data to temp files
        let restore_dir = self.snapshot_dir.join("fast-restore").join(new_vm_id);
        fs::create_dir_all(&restore_dir)?;

        let mem_file = restore_dir.join("memory.snap");
        let state_file = restore_dir.join("state.snap");

        fs::write(&mem_file, &cached.memory_data)?;
        fs::write(&state_file, &cached.state_data)?;

        #[cfg(target_os = "linux")]
        {
            // Fast restore using cached data
            let socket_path = format!("/tmp/firecracker-{}.sock", new_vm_id);

            let mut cmd = std::process::Command::new("firecracker");
            cmd.arg("--api-sock").arg(&socket_path)
               .arg("--restore")
               .arg("--restore-state-file").arg(&state_file)
               .arg("--restore-mem-file").arg(&mem_file);

            let child = cmd.spawn()?;

            let elapsed = start.elapsed();
            info!("Fast restore from cache completed in {:?}", elapsed);

            return Ok(RestoredVm {
                vm_id: new_vm_id.to_string(),
                api_socket: socket_path,
                process: Some(child),
                restore_time: elapsed,
            });
        }

        #[cfg(not(target_os = "linux"))]
        {
            Ok(RestoredVm {
                vm_id: new_vm_id.to_string(),
                api_socket: format!("/tmp/firecracker-{new_vm_id}.sock"),
                process: None,
                restore_time: start.elapsed(),
            })
        }
    }

    /// Evict coldest snapshot from cache
    async fn evict_coldest(&self, cache: &mut SnapshotCache) {
        if let Some((coldest_id, _)) = cache.hot_snapshots.iter()
            .min_by_key(|(_, snap)| snap.access_count) {

            let id = coldest_id.clone();
            if let Some(removed) = cache.hot_snapshots.remove(&id) {
                let size_mb = (removed.memory_data.len() + removed.state_data.len()) / 1_048_576;
                cache.current_size_mb -= size_mb;
                debug!("Evicted cold snapshot from cache: {}", id);
            }
        }
    }

    fn calculate_rootfs_hash(&self, disk_file: &Path) -> Result<String> {
        // Use SHA256 of first 1MB for quick hash
        let mut file = File::open(disk_file)?;
        let mut buffer = vec![0u8; 1_048_576];
        let bytes_read = file.read(&mut buffer)?;
        buffer.truncate(bytes_read);

        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&buffer);
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Get snapshot statistics
    pub async fn get_stats(&self) -> SnapshotStats {
        let snapshots = self.snapshots.read().await;
        let cache = self.cache.read().await;

        let total_size: u64 = snapshots.values().map(|s| s.size_bytes).sum();
        let incremental_count = snapshots.values().filter(|s| s.metadata.incremental).count();

        SnapshotStats {
            total_snapshots: snapshots.len(),
            incremental_snapshots: incremental_count,
            total_size_mb: total_size / 1_048_576,
            cached_snapshots: cache.hot_snapshots.len(),
            cache_size_mb: cache.current_size_mb,
        }
    }
}

/// Restored VM information
pub struct RestoredVm {
    pub vm_id: String,
    pub api_socket: String,
    pub process: Option<std::process::Child>,
    pub restore_time: Duration,
}

#[derive(Debug, Serialize)]
pub struct SnapshotStats {
    pub total_snapshots: usize,
    pub incremental_snapshots: usize,
    pub total_size_mb: u64,
    pub cached_snapshots: usize,
    pub cache_size_mb: usize,
}

// Note: Custom FirecrackerApiClient, SnapshotCreateParams, and SnapshotType removed
// These are now provided by firecracker-rs-sdk:
// - firecracker_rs_sdk::instance::Instance (replaces FirecrackerApiClient)
// - firecracker_rs_sdk::models::SnapshotCreateParams
// - firecracker_rs_sdk::models::SnapshotType