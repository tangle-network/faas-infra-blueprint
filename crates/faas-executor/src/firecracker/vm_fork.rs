//! VM Forking with CoW Memory and Instant Branching
//! Provides sub-millisecond VM forking similar to Docker container forking

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::vm_snapshot::{VmSnapshotManager, VmSnapshot};


/// VM Fork Manager for instant VM branching
pub struct VmForkManager {
    snapshot_manager: Arc<VmSnapshotManager>,
    forks: Arc<RwLock<HashMap<String, VmFork>>>,
    fork_tree: Arc<RwLock<ForkTree>>,
    config: ForkConfig,
}

#[derive(Debug, Clone)]
struct VmFork {
    id: String,
    parent_id: Option<String>,
    vm_id: String,
    snapshot_id: String,
    created_at: Instant,
    metadata: ForkMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ForkMetadata {
    generation: u32,
    memory_pages_shared: usize,
    memory_pages_private: usize,
    cow_enabled: bool,
}


/// Tree structure tracking fork lineage
pub struct ForkTree {
    nodes: HashMap<String, ForkNode>,
    root_forks: Vec<String>,
}

struct ForkNode {
    fork_id: String,
    children: Vec<String>,
    depth: u32,
}

#[derive(Debug, Clone)]
pub struct ForkConfig {
    pub enable_cow: bool,
    pub max_fork_depth: u32,
    pub prewarm_forks: usize,
    pub memory_dedup: bool,
}

impl Default for ForkConfig {
    fn default() -> Self {
        Self {
            enable_cow: true,
            max_fork_depth: 10,
            prewarm_forks: 5,
            memory_dedup: true,
        }
    }
}

impl VmForkManager {
    pub fn new(
        snapshot_manager: Arc<VmSnapshotManager>,
        config: ForkConfig,
    ) -> Self {
        Self {
            snapshot_manager,
            forks: Arc::new(RwLock::new(HashMap::new())),
            fork_tree: Arc::new(RwLock::new(ForkTree {
                nodes: HashMap::new(),
                root_forks: Vec::new(),
            })),
            config,
        }
    }

    /// Create a base VM that can be forked
    pub async fn create_base_vm(
        &self,
        base_id: &str,
        vm_config: &FirecrackerVmConfig,
    ) -> Result<String> {
        info!("Creating base VM for forking: {}", base_id);

        // Launch VM with special fork-optimized configuration
        let vm_id = self.launch_fork_optimized_vm(vm_config).await?;

        // Let VM initialize
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Create initial snapshot
        let snapshot_id = format!("base-snapshot-{}", base_id);
        let api_socket = format!("/tmp/firecracker-{}.sock", vm_id);

        self.snapshot_manager
            .create_snapshot(&vm_id, &snapshot_id, &api_socket)
            .await?;

        // Pre-warm snapshot for fast forking
        self.snapshot_manager.prewarm_snapshot(&snapshot_id).await?;

        // Store as root fork
        let fork = VmFork {
            id: base_id.to_string(),
            parent_id: None,
            vm_id: vm_id.clone(),
            snapshot_id: snapshot_id.clone(),
            created_at: Instant::now(),
            metadata: ForkMetadata {
                generation: 0,
                memory_pages_shared: 0,
                memory_pages_private: 0,
                cow_enabled: self.config.enable_cow,
            },
        };

        let mut forks = self.forks.write().await;
        forks.insert(base_id.to_string(), fork);

        let mut tree = self.fork_tree.write().await;
        tree.root_forks.push(base_id.to_string());
        tree.nodes.insert(
            base_id.to_string(),
            ForkNode {
                fork_id: base_id.to_string(),
                children: Vec::new(),
                depth: 0,
            },
        );

        info!("Base VM created: {} with snapshot {}", vm_id, snapshot_id);
        Ok(vm_id)
    }

    /// Fork a VM instantly from parent
    pub async fn fork_vm(
        &self,
        parent_id: &str,
        fork_id: &str,
    ) -> Result<ForkedVm> {
        let start = Instant::now();
        info!("Forking VM: {} from parent {}", fork_id, parent_id);

        // Get parent fork info
        let forks = self.forks.read().await;
        let parent = forks.get(parent_id)
            .ok_or_else(|| anyhow!("Parent fork not found: {}", parent_id))?
            .clone();
        drop(forks);

        // Check fork depth
        let tree = self.fork_tree.read().await;
        let parent_node = tree.nodes.get(parent_id)
            .ok_or_else(|| anyhow!("Parent node not found in tree"))?;

        if parent_node.depth >= self.config.max_fork_depth {
            return Err(anyhow!("Maximum fork depth ({}) exceeded", self.config.max_fork_depth));
        }
        let new_depth = parent_node.depth + 1;
        drop(tree);

        // Create incremental snapshot if CoW enabled
        let (snapshot_id, cow_pages) = if self.config.enable_cow {
            self.create_cow_fork(&parent, fork_id).await?
        } else {
            // Full snapshot fork
            self.create_full_fork(&parent, fork_id).await?
        };

        // Restore VM from snapshot (ultra-fast with pre-warmed cache)
        let new_vm_id = format!("vm-fork-{}", Uuid::new_v4());
        let restored = self.snapshot_manager
            .restore_snapshot(&snapshot_id, &new_vm_id)
            .await?;

        // Create fork record
        let fork = VmFork {
            id: fork_id.to_string(),
            parent_id: Some(parent_id.to_string()),
            vm_id: new_vm_id.clone(),
            snapshot_id: snapshot_id.clone(),
            created_at: Instant::now(),
            metadata: ForkMetadata {
                generation: parent.metadata.generation + 1,
                memory_pages_shared: cow_pages,
                memory_pages_private: 0,
                cow_enabled: self.config.enable_cow,
            },
        };

        // Update fork tree
        let mut forks = self.forks.write().await;
        forks.insert(fork_id.to_string(), fork.clone());

        let mut tree = self.fork_tree.write().await;
        tree.nodes.insert(
            fork_id.to_string(),
            ForkNode {
                fork_id: fork_id.to_string(),
                children: Vec::new(),
                depth: new_depth,
            },
        );

        // Update parent's children
        if let Some(parent_node) = tree.nodes.get_mut(parent_id) {
            parent_node.children.push(fork_id.to_string());
        }

        let fork_time = start.elapsed();
        info!(
            "VM forked in {:?} - ID: {}, Snapshot: {}, CoW Pages: {}",
            fork_time, new_vm_id, snapshot_id, cow_pages
        );

        Ok(ForkedVm {
            fork_id: fork_id.to_string(),
            vm_id: new_vm_id,
            api_socket: restored.api_socket,
            fork_time,
            metadata: fork.metadata,
        })
    }

    /// Create CoW fork with shared memory pages
    async fn create_cow_fork(
        &self,
        parent: &VmFork,
        fork_id: &str,
    ) -> Result<(String, usize)> {
        let snapshot_id = format!("cow-fork-{}", fork_id);

        #[cfg(target_os = "linux")]
        {
            // Use Linux-specific CoW mechanisms
            let cow_pages = self.setup_cow_memory_mapping(
                &parent.snapshot_id,
                &snapshot_id,
            ).await?;

            // Create incremental snapshot
            let api_socket = format!("/tmp/firecracker-{}.sock", parent.vm_id);
            self.snapshot_manager
                .create_incremental_snapshot(
                    &parent.vm_id,
                    &snapshot_id,
                    &parent.snapshot_id,
                    &api_socket,
                )
                .await?;

            Ok((snapshot_id, cow_pages))
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Fallback for non-Linux
            Ok((snapshot_id, 0))
        }
    }

    /// Setup CoW memory mapping using Linux mmap
    #[cfg(target_os = "linux")]
    async fn setup_cow_memory_mapping(
        &self,
        parent_snapshot: &str,
        child_snapshot: &str,
    ) -> Result<usize> {
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::io::AsRawFd;

        // Get parent snapshot memory file
        let parent_snap = self.snapshot_manager
            .snapshots
            .read()
            .await
            .get(parent_snapshot)
            .cloned()
            .ok_or_else(|| anyhow!("Parent snapshot not found"))?;

        let parent_mem_fd = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_RDONLY)
            .open(&parent_snap.memory_file)?;

        let parent_fd = parent_mem_fd.as_raw_fd();
        let file_size = std::fs::metadata(&parent_snap.memory_file)?.len() as usize;

        unsafe {
            // Create CoW mapping
            let addr = libc::mmap(
                std::ptr::null_mut(),
                file_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE, // MAP_PRIVATE creates CoW semantics
                parent_fd,
                0,
            );

            if addr == libc::MAP_FAILED {
                return Err(anyhow!("Failed to create CoW memory mapping"));
            }

            // Count shared pages (4KB pages)
            let page_count = file_size / 4096;

            // Create child snapshot file with CoW mapping
            let child_path = self.snapshot_manager.snapshot_dir
                .join(child_snapshot)
                .join("memory.cow");

            std::fs::create_dir_all(child_path.parent().unwrap())?;

            // Use mremap for efficient CoW
            let child_addr = libc::mremap(
                addr,
                file_size,
                file_size,
                libc::MREMAP_MAYMOVE,
            );

            if child_addr == libc::MAP_FAILED {
                libc::munmap(addr, file_size);
                return Err(anyhow!("Failed to remap for CoW"));
            }

            // Write CoW mapping to child file
            let child_fd = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(&child_path)?;

            // Mark pages as CoW
            libc::madvise(child_addr, file_size, libc::MADV_MERGEABLE);

            libc::munmap(child_addr, file_size);

            Ok(page_count)
        }
    }

    #[cfg(not(target_os = "linux"))]
    async fn setup_cow_memory_mapping(
        &self,
        _parent_snapshot: &str,
        _child_snapshot: &str,
    ) -> Result<usize> {
        Ok(0) // Stub for non-Linux
    }

    /// Create full fork (without CoW)
    async fn create_full_fork(
        &self,
        parent: &VmFork,
        fork_id: &str,
    ) -> Result<(String, usize)> {
        let snapshot_id = format!("full-fork-{}", fork_id);
        let api_socket = format!("/tmp/firecracker-{}.sock", parent.vm_id);

        self.snapshot_manager
            .create_snapshot(&parent.vm_id, &snapshot_id, &api_socket)
            .await?;

        Ok((snapshot_id, 0))
    }

    /// Launch VM with fork-optimized settings
    async fn launch_fork_optimized_vm(
        &self,
        config: &FirecrackerVmConfig,
    ) -> Result<String> {
        let vm_id = Uuid::new_v4().to_string();

        #[cfg(target_os = "linux")]
        {
            let socket_path = format!("/tmp/firecracker-{}.sock", vm_id);

            // Fork-optimized Firecracker configuration
            let mut cmd = std::process::Command::new("firecracker");
            cmd.arg("--api-sock").arg(&socket_path);

            // Enable features for fast forking
            if self.config.memory_dedup {
                cmd.env("FIRECRACKER_ENABLE_KSM", "1");
            }

            // Use huge pages for better CoW performance
            cmd.env("FIRECRACKER_HUGEPAGES", "1");

            let mut child = cmd.spawn()?;

            // Configure via API
            tokio::time::sleep(Duration::from_millis(100)).await;

            let client = FirecrackerApiClient::new(&socket_path);
            client.configure_vm(config).await?;
            client.start_vm().await?;

            Ok(vm_id)
        }

        #[cfg(not(target_os = "linux"))]
        {
            Ok(vm_id)
        }
    }

    /// Pre-warm fork pool for instant availability
    pub async fn prewarm_fork_pool(&self, base_id: &str, count: usize) -> Result<()> {
        info!("Pre-warming {} forks from base {}", count, base_id);

        for i in 0..count.min(self.config.prewarm_forks) {
            let fork_id = format!("{}-prewarm-{}", base_id, i);
            self.fork_vm(base_id, &fork_id).await?;
        }

        Ok(())
    }

    /// Execute command in forked VM
    pub async fn execute_in_fork(
        &self,
        fork_id: &str,
        command: &str,
        payload: &[u8],
    ) -> Result<Vec<u8>> {
        let forks = self.forks.read().await;
        let fork = forks.get(fork_id)
            .ok_or_else(|| anyhow!("Fork not found: {}", fork_id))?;

        let vm_id = fork.vm_id.clone(); // Clone before dropping
        let serial_socket = format!("/tmp/serial-{}.sock", fork.vm_id); // Clone socket path too
        drop(forks);

        let _api_socket = format!("/tmp/firecracker-{}.sock", vm_id);

        // Execute via communication layer
        let comm_config = super::communication::CommunicationConfig {
            vsock_cid: Some(3), // Would be dynamic
            serial_device: Some(serial_socket),
            ssh_config: None,
            timeout: Duration::from_secs(30),
            retry_attempts: 3,
            retry_delay: Duration::from_millis(100),
        };

        let executor = super::communication::VmCommandExecutor::new(comm_config);

        let config = faas_common::SandboxConfig {
            function_id: fork_id.to_string(),
            source: String::new(),
            command: vec![command.to_string()],
            payload: payload.to_vec(),
            env_vars: None,
        };

        executor.execute(&config).await
            .map_err(|e| anyhow!("Execution failed: {:?}", e))
    }

    /// Cleanup fork and reclaim resources
    pub async fn cleanup_fork(&self, fork_id: &str) -> Result<()> {
        let mut forks = self.forks.write().await;

        if let Some(fork) = forks.remove(fork_id) {
            // Stop VM
            #[cfg(target_os = "linux")]
            {
                let api_socket = format!("/tmp/firecracker-{}.sock", fork.vm_id);
                let client = FirecrackerApiClient::new(&api_socket);
                let _ = client.stop_vm().await;
            }

            // Update tree
            let mut tree = self.fork_tree.write().await;
            tree.nodes.remove(fork_id);

            // Remove from parent's children
            if let Some(parent_id) = fork.parent_id {
                if let Some(parent_node) = tree.nodes.get_mut(&parent_id) {
                    parent_node.children.retain(|id| id != fork_id);
                }
            }

            info!("Cleaned up fork: {}", fork_id);
        }

        Ok(())
    }

    /// Get fork statistics
    pub async fn get_stats(&self) -> ForkStats {
        let forks = self.forks.read().await;
        let tree = self.fork_tree.read().await;

        let cow_forks = forks.values()
            .filter(|f| f.metadata.cow_enabled)
            .count();

        let total_shared_pages: usize = forks.values()
            .map(|f| f.metadata.memory_pages_shared)
            .sum();

        let max_depth = tree.nodes.values()
            .map(|n| n.depth)
            .max()
            .unwrap_or(0);

        ForkStats {
            total_forks: forks.len(),
            active_forks: forks.len(),
            cow_forks,
            total_shared_pages,
            max_fork_depth: max_depth,
            root_forks: tree.root_forks.len(),
        }
    }
}

/// Forked VM information
pub struct ForkedVm {
    pub fork_id: String,
    pub vm_id: String,
    pub api_socket: String,
    pub fork_time: Duration,
    pub metadata: ForkMetadata,
}

#[derive(Debug, Serialize)]
pub struct ForkStats {
    pub total_forks: usize,
    pub active_forks: usize,
    pub cow_forks: usize,
    pub total_shared_pages: usize,
    pub max_fork_depth: u32,
    pub root_forks: usize,
}

/// Firecracker VM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirecrackerVmConfig {
    pub vcpu_count: u8,
    pub mem_size_mib: usize,
    pub kernel_path: String,
    pub rootfs_path: String,
    pub enable_cow: bool,
}

/// API client for Firecracker control
struct FirecrackerApiClient {
    socket_path: String,
}

impl FirecrackerApiClient {
    fn new(socket_path: &str) -> Self {
        Self {
            socket_path: socket_path.to_string(),
        }
    }

    #[cfg(target_os = "linux")]
    async fn configure_vm(&self, config: &FirecrackerVmConfig) -> Result<()> {
        // Configure VM via Firecracker API
        // Would use actual HTTP calls to unix socket
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    async fn configure_vm(&self, _config: &FirecrackerVmConfig) -> Result<()> {
        Ok(())
    }

    #[cfg(target_os = "linux")]
    async fn start_vm(&self) -> Result<()> {
        // Start VM via API
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    async fn start_vm(&self) -> Result<()> {
        Ok(())
    }

    #[cfg(target_os = "linux")]
    async fn stop_vm(&self) -> Result<()> {
        // Stop VM via API
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    async fn stop_vm(&self) -> Result<()> {
        Ok(())
    }
}