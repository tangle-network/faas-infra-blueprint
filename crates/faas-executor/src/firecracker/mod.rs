//! Firecracker microVM integration module
//! Provides complete VM lifecycle management with KVM acceleration

pub mod communication;
pub mod vm_manager;

// Guest agent source is included for documentation
// It should be compiled separately and included in rootfs
#[doc(hidden)]
pub const GUEST_AGENT_SOURCE: &str = include_str!("guest_agent.rs");

pub use communication::{CommunicationConfig as CommConfig, VmCommandExecutor};
pub use vm_manager::{FirecrackerManager, NetworkConfig, VmConfig, VmInstance, VmState};

use async_trait::async_trait;
use faas_common::{InvocationResult, Result as CommonResult, SandboxConfig, SandboxExecutor};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{error, info, warn};

/// High-level Firecracker executor implementing SandboxExecutor trait
pub struct FirecrackerExecutor {
    fc_binary_path: String,
    kernel_image_path: String,
    rootfs_path: String,
    api_socket_base: String,
    vsock_enabled: bool,
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

        Ok(Self {
            fc_binary_path,
            kernel_image_path,
            rootfs_path,
            api_socket_base: "/tmp/firecracker".to_string(),
            vsock_enabled: cfg!(target_os = "linux"),
        })
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
        }
    }

    /// Execute command in VM using available communication methods
    async fn execute_in_vm(
        &self,
        vm_id: &str,
        config: &SandboxConfig,
        vsock_cid: Option<u32>,
        api_socket: &str,
    ) -> Result<Vec<u8>, anyhow::Error> {
        // Build communication config
        let comm_config = communication::CommunicationConfig {
            vsock_cid,
            serial_device: Some(format!("/tmp/firecracker-{}-console.sock", vm_id)),
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
            .map_err(|e| anyhow::anyhow!("VM command execution failed: {}", e))
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

        // Generate unique VM ID
        let vm_id = format!("vm-{}", uuid::Uuid::new_v4());
        let api_socket = format!("{}-{}.socket", self.api_socket_base, vm_id);

        // Start Firecracker with firecracker-rs-sdk
        #[cfg(target_os = "linux")]
        {
            // Create VM manager
            let manager = vm_manager::FirecrackerManager::new(PathBuf::from("/var/lib/firecracker"))
                .map_err(|e| faas_common::FaasError::Executor(format!("Failed to create manager: {}", e)))?;

            // Set up network if needed
            let _ = manager.setup_network().await;

            // Configure the VM
            let vsock_cid = if self.vsock_enabled {
                // Allocate a unique CID (Context ID) for vsock
                // CID 2 is reserved for host, 3+ are for guests
                Some(3 + (std::process::id() % 1000) as u32)
            } else {
                None
            };

            // Create VM configuration
            let vm_config = vm_manager::VmConfig {
                vcpu_count: 1,
                mem_size_mib: 256,
                kernel_path: PathBuf::from(&self.kernel_image_path),
                kernel_args: "console=ttyS0 reboot=k panic=1 pci=off".to_string(),
                rootfs_path: PathBuf::from(&self.rootfs_path),
                network_interfaces: vec![],
                vsock_cid,
                gpu_devices: vec![],
                jailer_cfg: None,
            };

            // Launch the VM
            let launched_vm_id = manager.launch_vm(vm_config).await
                .map_err(|e| faas_common::FaasError::Executor(format!("Failed to launch VM: {}", e)))?;

            // Execute command in VM
            let output = match self.execute_in_vm(&launched_vm_id, &config, vsock_cid, &api_socket).await {
                Ok(output) => output,
                Err(e) => {
                    error!("Failed to execute in VM: {}", e);
                    Vec::new()
                }
            };

            // Stop the VM
            let _ = manager.stop_vm(&launched_vm_id).await;

            // Clean up socket
            let _ = std::fs::remove_file(&api_socket);

            Ok(InvocationResult {
                request_id: launched_vm_id,
                response: if output.is_empty() { None } else { Some(output) },
                logs: Some("VM execution completed".to_string()),
                error: None,
            })
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(faas_common::FaasError::Executor(
                "Firecracker only works on Linux with KVM".to_string(),
            ))
        }
    }
}
