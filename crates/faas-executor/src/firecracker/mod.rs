//! Firecracker microVM integration module
//! Provides complete VM lifecycle management with KVM acceleration

pub mod communication;
pub mod vm_cache;
pub mod vm_fork;
pub mod vm_manager;
pub mod vm_scaling;
pub mod vm_snapshot;

// Guest agent source is included for documentation
// It should be compiled separately and included in rootfs
#[doc(hidden)]
pub const GUEST_AGENT_SOURCE: &str = include_str!("guest_agent.rs");

pub use communication::{CommunicationConfig as CommConfig, VmCommandExecutor};
pub use vm_cache::{CacheConfig, VmResultCache as MultiLevelVmCache};
pub use vm_fork::{ForkTree, ForkedVm, VmForkManager};
pub use vm_manager::{FirecrackerManager, NetworkConfig, VmConfig, VmInstance, VmState};
pub use vm_scaling::{ScalingConfig, VmPool, VmPredictiveScaler};
pub use vm_snapshot::{RestoredVm, VmSnapshot, VmSnapshotManager};

#[cfg(target_os = "linux")]
use anyhow::anyhow;
use async_trait::async_trait;
use faas_common::{InvocationResult, Result as CommonResult, SandboxConfig, SandboxExecutor};
#[cfg(target_os = "linux")]
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
#[cfg(target_os = "linux")]
use tracing::info;
use tracing::warn;

/// High-level Firecracker executor implementing SandboxExecutor trait
pub struct FirecrackerExecutor {
    fc_binary_path: String,
    kernel_image_path: String,
    rootfs_path: String,
    api_socket_base: String,
    vsock_enabled: bool,
    vm_manager: Option<Arc<FirecrackerManager>>,
    snapshot_manager: Option<Arc<VmSnapshotManager>>,
    cache: Option<Arc<MultiLevelVmCache>>,
    fork_manager: Option<Arc<VmForkManager>>,
    scaler: Option<Arc<VmPredictiveScaler>>,
}

#[cfg(target_os = "linux")]
enum VmAcquisition {
    Warm {
        vm_id: String,
        environment: String,
    },
    Fork {
        handle: ForkedVm,
    },
    Cold {
        vm_id: String,
        vsock_hint: Option<u32>,
        vsock_path: Option<PathBuf>,
    },
}

impl FirecrackerExecutor {
    /// Create a new Firecracker executor
    pub fn new(
        fc_binary_path: String,
        kernel_image_path: String,
        rootfs_path: String,
    ) -> Result<Self, anyhow::Error> {
        // Check if KVM is available
        if !Self::check_kvm_available() {
            warn!("KVM not available, Firecracker will not work");
        }

        // Initialize optimization components only on Linux
        let (vm_manager, snapshot_manager, cache, fork_manager, scaler) =
            if cfg!(target_os = "linux") {
                let vm_mgr = match FirecrackerManager::new(PathBuf::from("/var/lib/firecracker")) {
                    Ok(mgr) => Arc::new(mgr),
                    Err(e) => {
                        warn!("Failed to initialize VM manager: {}", e);
                        return Ok(Self {
                            fc_binary_path,
                            kernel_image_path,
                            rootfs_path,
                            api_socket_base: "/tmp/firecracker".to_string(),
                            vsock_enabled: cfg!(target_os = "linux"),
                            vm_manager: None,
                            snapshot_manager: None,
                            cache: None,
                            fork_manager: None,
                            scaler: None,
                        });
                    }
                };

                let snapshot_mgr =
                    match VmSnapshotManager::new(PathBuf::from("/var/lib/firecracker/snapshots")) {
                        Ok(mgr) => Arc::new(mgr),
                        Err(e) => {
                            warn!("Failed to initialize snapshot manager: {}", e);
                            return Ok(Self {
                                fc_binary_path,
                                kernel_image_path,
                                rootfs_path,
                                api_socket_base: "/tmp/firecracker".to_string(),
                                vsock_enabled: cfg!(target_os = "linux"),
                                vm_manager: Some(vm_mgr),
                                snapshot_manager: None,
                                cache: None,
                                fork_manager: None,
                                scaler: None,
                            });
                        }
                    };

                let cache_config = CacheConfig {
                    max_size_bytes: 100 * 1024 * 1024, // 100MB
                    max_entries: 1000,
                    default_ttl: Some(Duration::from_secs(3600)),
                    compression_enabled: true,
                    eviction_policy: vm_cache::EvictionPolicy::Adaptive,
                };
                let vm_cache = Arc::new(MultiLevelVmCache::new(cache_config));

                let fork_mgr = VmForkManager::new(
                    snapshot_mgr.clone(),
                    vm_mgr.clone(),
                    vm_fork::ForkConfig::default(),
                );
                let fork_mgr = Arc::new(fork_mgr);

                let scaling_config = ScalingConfig {
                    min_warm_vms: 1,
                    max_warm_vms: 10,
                    scale_up_threshold: 0.8,
                    scale_down_threshold: 0.2,
                    prediction_window: Duration::from_secs(300),
                    warmup_time: Duration::from_secs(5),
                };
                let scaler = Arc::new(VmPredictiveScaler::new(
                    fork_mgr.clone(),
                    snapshot_mgr.clone(),
                    scaling_config,
                ));

                (
                    Some(vm_mgr),
                    Some(snapshot_mgr),
                    Some(vm_cache),
                    Some(fork_mgr),
                    Some(scaler),
                )
            } else {
                (None, None, None, None, None)
            };

        Ok(Self {
            fc_binary_path,
            kernel_image_path,
            rootfs_path,
            api_socket_base: "/tmp/firecracker".to_string(),
            vsock_enabled: cfg!(target_os = "linux"),
            vm_manager,
            snapshot_manager,
            cache,
            fork_manager,
            scaler,
        })
    }

    #[cfg(target_os = "linux")]
    async fn resolve_vsock_cid(&self, vm_id: &str) -> Option<u32> {
        let manager = self.vm_manager.as_ref()?.clone();
        let vm_arc = {
            let vms = manager.vms.read().await;
            vms.get(vm_id).cloned()
        };

        if let Some(vm_arc) = vm_arc {
            let vm = vm_arc.read().await;
            vm.vsock_cid
        } else {
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn vsock_path_for(&self, vm_id: &str) -> PathBuf {
        PathBuf::from(format!("{}-{}.vsock", self.api_socket_base, vm_id))
    }

    #[cfg(target_os = "linux")]
    async fn create_vm_snapshot(&self, vm_id: &str) -> anyhow::Result<()> {
        let snapshot_mgr = match &self.snapshot_manager {
            Some(mgr) => mgr.clone(),
            None => return Ok(()),
        };
        let manager = match &self.vm_manager {
            Some(mgr) => mgr.clone(),
            None => return Ok(()),
        };

        let vm_arc = {
            let vms = manager.vms.read().await;
            vms.get(vm_id).cloned()
        }
        .ok_or_else(|| anyhow!("VM {} not found for snapshotting", vm_id))?;

        let mut vm = vm_arc.write().await;
        let snapshot_id = format!("snap-{}", vm_id);
        snapshot_mgr
            .create_snapshot(vm_id, &snapshot_id, &mut vm.fc_instance)
            .await?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    async fn cleanup_acquisition(&self, acquisition: &VmAcquisition) {
        match acquisition {
            VmAcquisition::Warm { vm_id, environment } => {
                if let Some(ref scaler) = self.scaler {
                    let _ = scaler.release_vm(environment, vm_id).await;
                }
            }
            VmAcquisition::Fork { handle } => {
                if let Some(ref fork_mgr) = self.fork_manager {
                    let _ = fork_mgr.cleanup_fork(&handle.fork_id).await;
                }
                let _ = fs::remove_file(&handle.api_socket);
            }
            VmAcquisition::Cold {
                vm_id, vsock_path, ..
            } => {
                if let Some(ref manager) = self.vm_manager {
                    let _ = manager.stop_vm(vm_id).await;
                }
                let path = vsock_path
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| self.vsock_path_for(vm_id));
                let _ = fs::remove_file(&path);
            }
        }
    }

    /// Check if KVM is available on the system
    fn check_kvm_available() -> bool {
        #[cfg(target_os = "linux")]
        {
            std::path::Path::new("/dev/kvm").exists()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Create a stub executor for environments without KVM
    pub fn stub() -> Self {
        Self {
            fc_binary_path: String::new(),
            kernel_image_path: String::new(),
            rootfs_path: String::new(),
            api_socket_base: String::new(),
            vsock_enabled: false,
            vm_manager: None,
            snapshot_manager: None,
            cache: None,
            fork_manager: None,
            scaler: None,
        }
    }

    /// Cold start a VM
    async fn cold_start_vm(
        &self,
        vm_id: &str,
        config: &SandboxConfig,
    ) -> Result<(String, Option<u32>, Option<PathBuf>), faas_common::FaasError> {
        #[cfg(target_os = "linux")]
        {
            use vm_manager::VsockDevice;

            let manager = self
                .vm_manager
                .as_ref()
                .ok_or_else(|| {
                    faas_common::FaasError::Executor(
                        "Firecracker manager not initialized".to_string(),
                    )
                })?
                .clone();

            // Best-effort network setup
            let _ = manager.setup_network().await;

            // Configure vsock if enabled
            let (vsock_cid, vsock_device, vsock_path_option) = if self.vsock_enabled {
                let cid = 3 + (std::process::id() % 1000) as u32;
                let vsock_path = self.vsock_path_for(vm_id);
                if let Some(parent) = vsock_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::remove_file(&vsock_path);

                (
                    Some(cid),
                    Some(VsockDevice {
                        guest_cid: cid,
                        uds_path: vsock_path.to_string_lossy().into_owned(),
                    }),
                    Some(vsock_path),
                )
            } else {
                (None, None, None)
            };

            let vm_config = vm_manager::VmConfig {
                vcpu_count: 1,
                mem_size_mib: 256,
                kernel_path: PathBuf::from(&self.kernel_image_path),
                kernel_args: "console=ttyS0 reboot=k panic=1 pci=off".to_string(),
                rootfs_path: PathBuf::from(&self.rootfs_path),
                network_interfaces: vec![],
                vsock: vsock_device,
                enable_jailer: false,
                jailer_cfg: None,
            };

            let launched_vm_id = manager.launch_vm(vm_config).await.map_err(|e| {
                faas_common::FaasError::Executor(format!("Failed to launch VM: {}", e))
            })?;

            let resolved_vsock = self.resolve_vsock_cid(&launched_vm_id).await;

            Ok((launched_vm_id, resolved_vsock, vsock_path_option))
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(faas_common::FaasError::Executor(
                "Firecracker only works on Linux with KVM".to_string(),
            ))
        }
    }

    /// Execute command in VM using available communication methods
    async fn execute_in_vm(
        &self,
        vm_id: &str,
        config: &SandboxConfig,
        vsock_hint: Option<u32>,
    ) -> Result<Vec<u8>, anyhow::Error> {
        #[cfg(target_os = "linux")]
        {
            if let Some(ref manager) = self.vm_manager {
                let vms = manager.vms.read().await;
                if let Some(vm_arc) = vms.get(vm_id) {
                    drop(vms);
                    match manager.execute_in_vm(vm_id, config).await {
                        Ok(output) => return Ok(output),
                        Err(error) => {
                            warn!(
                                "Manager execution path failed for VM {}: {}. Falling back to direct communication.",
                                vm_id, error
                            );
                        }
                    }
                } else {
                    drop(vms);
                }
            }
        }

        #[cfg(target_os = "linux")]
        let vsock_cid = if let Some(cid) = vsock_hint {
            Some(cid)
        } else {
            self.resolve_vsock_cid(vm_id).await
        };

        #[cfg(not(target_os = "linux"))]
        let vsock_cid = vsock_hint;

        // Build communication config
        let comm_config = communication::CommunicationConfig {
            vsock_cid,
            serial_device: Some(format!("/tmp/firecracker-{vm_id}-console.sock")),
            ssh_config: None, // Could be configured if SSH is set up in the VM
            timeout: Duration::from_secs(30),
            retry_attempts: 3,
            retry_delay: Duration::from_millis(100),
        };

        // Create executor
        let executor = VmCommandExecutor::new(comm_config);

        // Execute command
        executor
            .execute(config)
            .await
            .map_err(|e| anyhow::anyhow!("VM command execution failed: {e}"))
    }

    /// Execute with VM forking for branched execution
    pub async fn execute_branched(
        &self,
        config: SandboxConfig,
        parent_vm_id: &str,
    ) -> CommonResult<InvocationResult> {
        // Check KVM availability
        if !Self::check_kvm_available() {
            return Err(faas_common::FaasError::Executor(
                "KVM not available on this system".to_string(),
            ));
        }

        #[cfg(target_os = "linux")]
        {
            // Generate unique fork ID
            let fork_id = format!("vm-fork-{}", uuid::Uuid::new_v4());
            // Use fork manager if available
            if let Some(ref fork_mgr) = self.fork_manager {
                info!("Forking VM from parent: {}", parent_vm_id);

                // Fork the VM
                let forked = fork_mgr
                    .fork_vm(parent_vm_id, &fork_id)
                    .await
                    .map_err(|e| {
                        faas_common::FaasError::Executor(format!("Failed to fork VM: {}", e))
                    })?;

                info!("Created VM fork {} from parent {}", fork_id, parent_vm_id);

                // Execute in forked VM
                let output = match self.execute_in_vm(&forked.vm_id, &config, None).await {
                    Ok(output) => output,
                    Err(e) => {
                        error!("Failed to execute in forked VM: {}", e);

                        // Clean up fork
                        let _ = fork_mgr.cleanup_fork(&fork_id).await;

                        return Err(faas_common::FaasError::Executor(format!(
                            "Fork execution failed: {}",
                            e
                        )));
                    }
                };

                // Clean up socket
                let _ = std::fs::remove_file(&forked.api_socket);

                Ok(InvocationResult {
                    request_id: fork_id,
                    response: if output.is_empty() {
                        None
                    } else {
                        Some(output)
                    },
                    logs: Some(format!(
                        "VM fork execution completed (parent: {})",
                        parent_vm_id
                    )),
                    error: None,
                })
            } else {
                // Fall back to snapshot-based branching if fork manager unavailable
                if let Some(ref snapshot_mgr) = self.snapshot_manager {
                    info!(
                        "Using snapshot-based branching from parent: {}",
                        parent_vm_id
                    );

                    // Check for parent snapshot
                    let parent_snapshot_id = format!("snap-{}", parent_vm_id);

                    // Try to restore from parent snapshot
                    match snapshot_mgr
                        .restore_snapshot(&parent_snapshot_id, &fork_id)
                        .await
                    {
                        Ok(restored) => {
                            info!("Restored VM {} from parent snapshot", fork_id);

                            // Execute in restored VM
                            let output =
                                match self.execute_in_vm(&restored.vm_id, &config, None).await {
                                    Ok(output) => output,
                                    Err(e) => {
                                        error!("Failed to execute in restored VM: {}", e);
                                        Vec::new()
                                    }
                                };

                            Ok(InvocationResult {
                                request_id: fork_id,
                                response: if output.is_empty() {
                                    None
                                } else {
                                    Some(output)
                                },
                                logs: Some(format!(
                                    "VM snapshot branch execution completed (parent: {})",
                                    parent_vm_id
                                )),
                                error: None,
                            })
                        }
                        Err(e) => {
                            warn!("Failed to restore from parent snapshot: {}", e);
                            // Fall back to regular execution
                            self.execute(config).await
                        }
                    }
                } else {
                    // No forking available, fall back to regular execution
                    warn!("VM forking not available, using regular execution");
                    self.execute(config).await
                }
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(faas_common::FaasError::Executor(
                "Firecracker VM forking only works on Linux with KVM".to_string(),
            ))
        }
    }
}

#[async_trait]
impl SandboxExecutor for FirecrackerExecutor {
    async fn execute(&self, config: SandboxConfig) -> CommonResult<InvocationResult> {
        // Check KVM availability
        if !Self::check_kvm_available() {
            return Err(faas_common::FaasError::Executor(
                "KVM not available on this system".to_string(),
            ));
        }

        // Generate unique VM ID and cache key
        let vm_id = format!("vm-{}", uuid::Uuid::new_v4());
        // Create cache key from config
        let cache_key = format!(
            "{}:{}:{}",
            config.function_id.clone(),
            config.source.clone(),
            format!("{:x}", md5::compute(config.command.join(" ")))
        );

        // Start Firecracker with optimizations
        #[cfg(target_os = "linux")]
        {
            if let Some(ref cache) = self.cache {
                if let Ok(Some(cached)) = cache.get(&cache_key).await {
                    info!("Cache hit for key: {}", cache_key);
                    return Ok(InvocationResult {
                        request_id: vm_id,
                        response: cached.response,
                        logs: Some(format!(
                            "VM execution cached (hit rate: {:.2}%)",
                            cached.hit_rate
                        )),
                        error: cached.error,
                    });
                }
            }

            let environment_key = config.function_id.clone();

            let mut acquisition: Option<VmAcquisition> = None;

            if let Some(ref scaler) = self.scaler {
                scaler.record_request(&environment_key).await;
                if let Ok(acquired) = scaler.acquire_vm(&environment_key).await {
                    acquisition = Some(VmAcquisition::Warm {
                        vm_id: acquired.vm_id,
                        environment: environment_key.clone(),
                    });
                }
            }

            if acquisition.is_none() {
                if let Some(ref fork_mgr) = self.fork_manager {
                    if let Ok(fork) = fork_mgr.fork_vm("base-vm", &vm_id).await {
                        acquisition = Some(VmAcquisition::Fork { handle: fork });
                    }
                }
            }

            let acquisition = if let Some(acq) = acquisition {
                acq
            } else {
                let (cold_vm_id, vsock_hint, vsock_path) =
                    self.cold_start_vm(&vm_id, &config).await?;
                VmAcquisition::Cold {
                    vm_id: cold_vm_id,
                    vsock_hint,
                    vsock_path,
                }
            };

            let (target_vm_id, mut vsock_hint, was_warm) = match &acquisition {
                VmAcquisition::Warm { vm_id, .. } => (vm_id.clone(), None, true),
                VmAcquisition::Fork { handle } => (handle.vm_id.clone(), None, true),
                VmAcquisition::Cold {
                    vm_id, vsock_hint, ..
                } => (vm_id.clone(), *vsock_hint, false),
            };

            if vsock_hint.is_none() {
                vsock_hint = self.resolve_vsock_cid(&target_vm_id).await;
            }

            let output = match self.execute_in_vm(&target_vm_id, &config, vsock_hint).await {
                Ok(output) => output,
                Err(error) => {
                    error!("Failed to execute in VM {}: {}", target_vm_id, error);
                    self.cleanup_acquisition(&acquisition).await;
                    return Err(faas_common::FaasError::Executor(format!(
                        "VM execution failed: {}",
                        error
                    )));
                }
            };

            let result = InvocationResult {
                request_id: target_vm_id.clone(),
                response: if output.is_empty() {
                    None
                } else {
                    Some(output.clone())
                },
                logs: Some(format!(
                    "VM execution completed ({})",
                    if was_warm { "warm" } else { "cold" }
                )),
                error: None,
            };

            if let Some(ref cache) = self.cache {
                let cache_result = vm_cache::CacheResult {
                    response: result.response.clone(),
                    error: result.error.clone(),
                    hit_rate: 0.0,
                    cache_level: "L1".to_string(),
                };
                let _ = cache.put(cache_key, cache_result).await;
            }

            if !was_warm {
                if let Err(snapshot_err) = self.create_vm_snapshot(&target_vm_id).await {
                    warn!(
                        "Failed to create snapshot for VM {}: {}",
                        target_vm_id, snapshot_err
                    );
                }
            }

            self.cleanup_acquisition(&acquisition).await;

            Ok(result)
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(faas_common::FaasError::Executor(
                "Firecracker only works on Linux with KVM".to_string(),
            ))
        }
    }
}
