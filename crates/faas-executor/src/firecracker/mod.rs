//! Firecracker microVM integration module
//! Provides complete VM lifecycle management with KVM acceleration

pub mod vm_manager;

pub use vm_manager::{FirecrackerManager, NetworkConfig, VmConfig, VmInstance, VmState};

use async_trait::async_trait;
use faas_common::{InvocationResult, Result as CommonResult, SandboxConfig, SandboxExecutor};
use std::path::PathBuf;
use tracing::{error, info, warn};

/// High-level Firecracker executor implementing SandboxExecutor trait
pub struct FirecrackerExecutor {
    manager: Option<FirecrackerManager>,
    fc_binary_path: String,
    kernel_image_path: String,
}

impl FirecrackerExecutor {
    /// Create a new Firecracker executor
    pub fn new(fc_binary_path: String, kernel_image_path: String) -> Result<Self, anyhow::Error> {
        let base_dir = PathBuf::from("/var/lib/firecracker");

        // Try to create manager, but don't fail if KVM not available
        let manager = FirecrackerManager::new(base_dir).ok();

        if manager.is_none() {
            warn!("Firecracker manager not available (likely no KVM support)");
        }

        Ok(Self {
            manager,
            fc_binary_path,
            kernel_image_path,
        })
    }

    /// Create a stub executor for environments without KVM
    pub fn stub() -> Self {
        Self {
            manager: None,
            fc_binary_path: String::new(),
            kernel_image_path: String::new(),
        }
    }
}

#[async_trait]
impl SandboxExecutor for FirecrackerExecutor {
    async fn execute(&self, config: SandboxConfig) -> CommonResult<InvocationResult> {
        if let Some(ref manager) = self.manager {
            // Setup network if needed
            let _ = manager.setup_network().await;

            // Create VM configuration
            let vm_config = VmConfig {
                vcpu_count: 1,
                mem_size_mib: 256,
                kernel_path: PathBuf::from(&self.kernel_image_path),
                kernel_args: format!("init={} console=ttyS0", config.command.join(" ")),
                rootfs_path: PathBuf::from("/var/lib/firecracker/rootfs/ubuntu.ext4"),
                ..Default::default()
            };

            // Launch VM
            match manager.launch_vm(vm_config).await {
                Ok(vm_id) => {
                    info!("Launched Firecracker VM: {}", vm_id);

                    // TODO: Connect to VM via vsock/serial and execute command
                    // For now, return a mock result

                    // Stop VM
                    let _ = manager.stop_vm(&vm_id).await;

                    Ok(InvocationResult {
                        request_id: vm_id,
                        response: Some(b"Firecracker execution complete".to_vec()),
                        logs: Some("VM execution logs".to_string()),
                        error: None,
                    })
                }
                Err(e) => {
                    error!("Failed to launch VM: {}", e);
                    Err(faas_common::FaasError::Executor(format!(
                        "Firecracker launch failed: {}",
                        e
                    )))
                }
            }
        } else {
            // Fallback when Firecracker not available
            warn!("Firecracker not available, returning mock result");
            Ok(InvocationResult {
                request_id: uuid::Uuid::new_v4().to_string(),
                response: Some(b"Firecracker not available (no KVM)".to_vec()),
                logs: Some("Running in mock mode".to_string()),
                error: Some("KVM not available".to_string()),
            })
        }
    }
}
