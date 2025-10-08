//! Firecracker VM Manager with full microVM orchestration using firecracker-rs-sdk
//! This provides complete Firecracker integration with proper SDK usage

use anyhow::{Context, Result};
use firecracker_rs_sdk::{
    firecracker::FirecrackerOption,
    instance::Instance as FcInstance,
    models::{
        BootSource, Drive, MachineConfiguration, NetworkInterface as FcNetworkInterface, Vsock as FcVsock,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

/// Firecracker VM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub vcpu_count: u8,
    pub mem_size_mib: usize,
    pub kernel_path: PathBuf,
    pub kernel_args: String,
    pub rootfs_path: PathBuf,
    pub network_interfaces: Vec<NetworkInterface>,
    pub vsock: Option<VsockDevice>,
    pub enable_jailer: bool,
    pub jailer_cfg: Option<JailerConfig>,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            vcpu_count: 2,
            mem_size_mib: 512,
            kernel_path: PathBuf::from("/var/lib/faas/kernel/vmlinux.bin"),
            kernel_args: "console=ttyS0 reboot=k panic=1 pci=off".to_string(),
            rootfs_path: PathBuf::from("/var/lib/firecracker/rootfs/ubuntu.ext4"),
            network_interfaces: vec![],
            vsock: None,
            enable_jailer: false,
            jailer_cfg: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub iface_id: String,
    pub host_dev_name: String,
    pub guest_mac: String,
    pub rx_rate_limiter: Option<RateLimiter>,
    pub tx_rate_limiter: Option<RateLimiter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiter {
    pub bandwidth: Option<u64>,
    pub ops: Option<u64>,
    pub burst: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsockDevice {
    pub guest_cid: u32,
    pub uds_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JailerConfig {
    pub id: String,
    pub exec_file: PathBuf,
    pub uid: u32,
    pub gid: u32,
    pub chroot_base_dir: PathBuf,
    pub daemonize: bool,
}

/// Represents a running Firecracker VM with SDK instance
pub struct VmInstance {
    pub id: String,
    pub config: VmConfig,
    pub fc_instance: FcInstance,
    pub api_socket: PathBuf,
    pub metrics: VmMetrics,
    pub state: VmState,
    pub vsock_cid: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct VmMetrics {
    pub start_time: std::time::Instant,
    pub cpu_usage_ms: u64,
    pub memory_usage_bytes: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmState {
    Creating,
    Running,
    Paused,
    Stopped,
    Failed(String),
}

/// Firecracker VM Manager
pub struct FirecrackerManager {
    /// Base directory for VM data
    base_dir: PathBuf,
    /// Binary path to firecracker
    firecracker_bin: PathBuf,
    /// Binary path to jailer (optional)
    jailer_bin: Option<PathBuf>,
    /// Running VMs (public for snapshot/fork operations)
    pub(crate) vms: Arc<RwLock<HashMap<String, Arc<RwLock<VmInstance>>>>>,
    /// Network configuration
    network_cfg: NetworkConfig,
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub bridge_name: String,
    pub subnet: String,
    pub gateway: String,
    pub dns: Vec<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            bridge_name: "fcbr0".to_string(),
            subnet: "172.16.0.0/24".to_string(),
            gateway: "172.16.0.1".to_string(),
            dns: vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
        }
    }
}

impl FirecrackerManager {
    /// Create a new Firecracker manager
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&base_dir)?;

        // Find firecracker binary
        let firecracker_bin = Self::find_firecracker()?;
        let jailer_bin = Self::find_jailer().ok();

        // Check for KVM
        if !Path::new("/dev/kvm").exists() {
            warn!("KVM not available - Firecracker will not function properly");
            return Err(anyhow::anyhow!("KVM not available"));
        }

        // Check KVM permissions
        let kvm_meta = fs::metadata("/dev/kvm")?;
        use std::os::unix::fs::PermissionsExt;
        let mode = kvm_meta.permissions().mode();
        if mode & 0o666 != 0o666 {
            warn!("KVM device may not have proper permissions");
        }

        Ok(Self {
            base_dir,
            firecracker_bin,
            jailer_bin,
            vms: Arc::new(RwLock::new(HashMap::new())),
            network_cfg: NetworkConfig::default(),
        })
    }

    fn find_firecracker() -> Result<PathBuf> {
        let paths = [
            "/usr/local/bin/firecracker",
            "/usr/bin/firecracker",
            "/opt/firecracker/firecracker",
            "./firecracker",
        ];

        for path in &paths {
            let p = PathBuf::from(path);
            if p.exists() {
                // Verify it's executable
                let output = Command::new(&p).arg("--version").output();

                if let Ok(out) = output {
                    if out.status.success() {
                        let version = String::from_utf8_lossy(&out.stdout);
                        info!("Found Firecracker: {} - {}", path, version.trim());
                        return Ok(p);
                    }
                }
            }
        }

        Err(anyhow::anyhow!("Firecracker binary not found"))
    }

    fn find_jailer() -> Result<PathBuf> {
        let paths = [
            "/usr/local/bin/jailer",
            "/usr/bin/jailer",
            "/opt/firecracker/jailer",
            "./jailer",
        ];

        for path in &paths {
            let p = PathBuf::from(path);
            if p.exists() {
                info!("Found Jailer: {}", path);
                return Ok(p);
            }
        }

        Err(anyhow::anyhow!("Jailer binary not found"))
    }

    /// Setup network for VMs
    pub async fn setup_network(&self) -> Result<()> {
        info!(
            "Setting up Firecracker network bridge: {}",
            self.network_cfg.bridge_name
        );

        // Create bridge
        let _ = Command::new("ip")
            .args([
                "link",
                "add",
                &self.network_cfg.bridge_name,
                "type",
                "bridge",
            ])
            .output();

        // Set bridge up
        Command::new("ip")
            .args(["link", "set", &self.network_cfg.bridge_name, "up"])
            .output()?;

        // Add IP to bridge
        let _ = Command::new("ip")
            .args([
                "addr",
                "add",
                &format!("{}/24", self.network_cfg.gateway),
                "dev",
                &self.network_cfg.bridge_name,
            ])
            .output();

        // Enable IP forwarding
        fs::write("/proc/sys/net/ipv4/ip_forward", "1")?;

        // Setup NAT
        let _ = Command::new("iptables")
            .args([
                "-t",
                "nat",
                "-A",
                "POSTROUTING",
                "-s",
                &self.network_cfg.subnet,
                "!",
                "-d",
                &self.network_cfg.subnet,
                "-j",
                "MASQUERADE",
            ])
            .output();

        info!("Network setup complete");
        Ok(())
    }

    /// Create a TAP device for a VM
    fn create_tap_device(&self, vm_id: &str) -> Result<String> {
        let tap_name = format!("fc-tap-{}", &vm_id[..8]);

        // Create TAP device
        Command::new("ip")
            .args(["tuntap", "add", &tap_name, "mode", "tap"])
            .output()
            .context("Failed to create TAP device")?;

        // Set TAP up
        Command::new("ip")
            .args(["link", "set", &tap_name, "up"])
            .output()?;

        // Add to bridge
        Command::new("ip")
            .args([
                "link",
                "set",
                &tap_name,
                "master",
                &self.network_cfg.bridge_name,
            ])
            .output()?;

        Ok(tap_name)
    }

    /// Launch a new VM using firecracker-rs-sdk
    pub async fn launch_vm(&self, mut config: VmConfig) -> Result<String> {
        let vm_id = Uuid::new_v4().to_string();
        let vm_dir = self.base_dir.join(&vm_id);
        fs::create_dir_all(&vm_dir)?;

        info!("Launching Firecracker VM via SDK: {}", vm_id);

        // Verify kernel and rootfs exist
        if !config.kernel_path.exists() {
            return Err(anyhow::anyhow!(
                "Kernel not found: {:?}",
                config.kernel_path
            ));
        }
        if !config.rootfs_path.exists() {
            return Err(anyhow::anyhow!(
                "Rootfs not found: {:?}",
                config.rootfs_path
            ));
        }

        // Create API socket
        let api_socket = vm_dir.join("firecracker.sock");

        // Setup networking if not configured
        if config.network_interfaces.is_empty() {
            let tap_name = self.create_tap_device(&vm_id)?;
            config.network_interfaces.push(NetworkInterface {
                iface_id: "eth0".to_string(),
                host_dev_name: tap_name,
                guest_mac: Self::generate_mac(),
                rx_rate_limiter: None,
                tx_rate_limiter: None,
            });
        }

        // Build Firecracker instance using SDK
        let mut fc_opt = FirecrackerOption::new(&self.firecracker_bin);
        fc_opt
            .api_sock(&api_socket)
            .id(&vm_id)
            .log_path(Some(vm_dir.join("firecracker.log")));

        // Create SDK instance
        let mut fc_instance = fc_opt
            .build()
            .context("Failed to create Firecracker instance")?;

        // Start VMM process (SDK methods are synchronous)
        fc_instance
            .start_vmm()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start VMM process: {e:?}"))?;

        info!("VMM started for {}, configuring via SDK API", vm_id);

        // Wait for API socket
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Configure machine using SDK models
        let machine_config = MachineConfiguration {
            vcpu_count: config.vcpu_count as isize,
            mem_size_mib: config.mem_size_mib as isize,
            smt: Some(false),
            cpu_template: None,
            track_dirty_pages: Some(true), // Enable for snapshots
            huge_pages: None,
        };

        fc_instance
            .put_machine_configuration(&machine_config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to configure machine: {e:?}"))?;

        // Configure boot source using SDK models
        let boot_source = BootSource {
            kernel_image_path: config.kernel_path.clone(),
            boot_args: Some(config.kernel_args.clone()),
            initrd_path: None,
        };

        fc_instance
            .put_guest_boot_source(&boot_source)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to configure boot source: {e:?}"))?;

        // Configure rootfs drive using SDK models
        let rootfs_drive = Drive {
            drive_id: "rootfs".to_string(),
            path_on_host: config.rootfs_path.clone(),
            is_root_device: true,
            is_read_only: false,
            partuuid: None,
            cache_type: None,
            rate_limiter: None,
            io_engine: None,
            socket: None,
        };

        fc_instance
            .put_guest_drive_by_id(&rootfs_drive)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to configure rootfs drive: {e:?}"))?;

        // Configure network interfaces using SDK models
        for (idx, net_iface) in config.network_interfaces.iter().enumerate() {
            let fc_net_iface = FcNetworkInterface {
                iface_id: net_iface.iface_id.clone(),
                host_dev_name: net_iface.host_dev_name.clone().into(),
                guest_mac: Some(net_iface.guest_mac.clone()),
                rx_rate_limiter: None,
                tx_rate_limiter: None,
            };

            fc_instance
                .put_guest_network_interface_by_id(&fc_net_iface)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to configure network interface {idx}: {e:?}"))?;
        }

        // Configure vsock if requested
        let vsock_cid = 3 + (self.vms.read().await.len() as u32);
        if let Some(ref vsock_cfg) = config.vsock {
            let fc_vsock = FcVsock {
                guest_cid: vsock_cfg.guest_cid,
                uds_path: vsock_cfg.uds_path.clone().into(),
                vsock_id: None,
            };

            fc_instance
                .put_guest_vsock(&fc_vsock)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to configure vsock: {e:?}"))?;
        }

        // Start the VM using SDK
        fc_instance.start()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start VM: {e:?}"))?;

        info!("VM {} started successfully via SDK", vm_id);

        // Create VM instance with SDK handle
        let instance = VmInstance {
            id: vm_id.clone(),
            config,
            fc_instance,
            api_socket,
            metrics: VmMetrics {
                start_time: std::time::Instant::now(),
                cpu_usage_ms: 0,
                memory_usage_bytes: 0,
                network_rx_bytes: 0,
                network_tx_bytes: 0,
            },
            state: VmState::Running,
            vsock_cid: Some(vsock_cid),
        };

        // Store VM
        let mut vms = self.vms.write().await;
        vms.insert(vm_id.clone(), Arc::new(RwLock::new(instance)));

        info!("VM {} fully operational", vm_id);
        Ok(vm_id)
    }

    // Note: Obsolete methods removed - SDK handles all API communication directly
    // - create_jailer_command: SDK has built-in jailer support via JailerOption
    // - create_vm_config_json: SDK uses API calls, not config files
    // - configure_vm_api: Replaced by SDK instance methods (put_machine_configuration, etc.)
    // - api_request: SDK Instance has direct HTTP client for unix socket communication

    fn generate_mac() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        format!(
            "02:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            rng.gen::<u8>(),
            rng.gen::<u8>(),
            rng.gen::<u8>(),
            rng.gen::<u8>(),
            rng.gen::<u8>()
        )
    }

    pub async fn execute_in_vm(&self, vm_id: &str, config: &faas_common::SandboxConfig) -> Result<Vec<u8>> {
        // Execute command in VM via serial console
        let vms = self.vms.read().await;

        if let Some(vm_arc) = vms.get(vm_id) {
            let vm = vm_arc.read().await;

            if vm.state != VmState::Running {
                return Err(anyhow::anyhow!("VM {vm_id} is not running"));
            }

            // Use real vsock/serial communication instead of simulation
            let comm_config = crate::firecracker::communication::CommunicationConfig {
                vsock_cid: vm.vsock_cid,
                serial_device: Some(format!("/tmp/firecracker-{vm_id}-console.sock")),
                ssh_config: None,
                timeout: std::time::Duration::from_secs(30),
                retry_attempts: 3,
                retry_delay: std::time::Duration::from_millis(100),
            };

            let executor = crate::firecracker::communication::VmCommandExecutor::new(comm_config);

            // Execute command via real VM communication
            match executor.execute(config).await {
                Ok(output) => {
                    info!("Command executed successfully in VM {} via real communication", vm_id);
                    Ok(output)
                },
                Err(e) => {
                    warn!("VM communication failed for {}, error: {}", vm_id, e);
                    // Return error instead of fallback simulation
                    Err(anyhow::anyhow!("VM communication failed: {e}"))
                }
            }
        } else {
            Err(anyhow::anyhow!("VM {vm_id} not found"))
        }
    }

    /// Stop a VM using SDK
    pub async fn stop_vm(&self, vm_id: &str) -> Result<()> {
        let vms = self.vms.read().await;

        if let Some(vm_arc) = vms.get(vm_id) {
            let mut vm = vm_arc.write().await;

            // Use SDK's stop method
            vm.fc_instance.stop()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to stop VM via SDK: {e:?}"))?;

            // Wait a bit for graceful shutdown
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            vm.state = VmState::Stopped;
            info!("VM {} stopped via SDK", vm_id);
        }

        Ok(())
    }

    /// List all VMs
    pub async fn list_vms(&self) -> Vec<(String, VmState)> {
        let vms = self.vms.read().await;
        let mut result = Vec::new();

        for (id, vm_arc) in vms.iter() {
            let vm = vm_arc.read().await;
            result.push((id.clone(), vm.state.clone()));
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    #[ignore = "Requires KVM and Firecracker"]
    async fn test_firecracker_manager() {
        let temp_dir = TempDir::new().unwrap();
        let manager = FirecrackerManager::new(temp_dir.path().to_path_buf());

        if let Ok(mgr) = manager {
            // Setup network
            let _ = mgr.setup_network().await;

            // Try to launch a VM
            let config = VmConfig::default();
            let result = mgr.launch_vm(config).await;

            if let Ok(vm_id) = result {
                println!("VM launched: {}", vm_id);

                // List VMs
                let vms = mgr.list_vms().await;
                assert!(!vms.is_empty());

                // Stop VM
                let _ = mgr.stop_vm(&vm_id).await;
            }
        }
    }
}
