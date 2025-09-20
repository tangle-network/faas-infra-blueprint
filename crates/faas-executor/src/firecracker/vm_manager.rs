//! Firecracker VM Manager with full microVM orchestration
//! This provides complete Firecracker integration with proper error handling

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio, Child};
use std::fs::{self, File};
use std::io::Write;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};
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

/// Represents a running Firecracker VM
pub struct VmInstance {
    pub id: String,
    pub config: VmConfig,
    pub process: Option<Child>,
    pub api_socket: PathBuf,
    pub metrics: VmMetrics,
    pub state: VmState,
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
    /// Running VMs
    vms: Arc<RwLock<HashMap<String, Arc<RwLock<VmInstance>>>>>,
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
                let output = Command::new(&p)
                    .arg("--version")
                    .output();

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
        info!("Setting up Firecracker network bridge: {}", self.network_cfg.bridge_name);

        // Create bridge
        let _ = Command::new("ip")
            .args(&["link", "add", &self.network_cfg.bridge_name, "type", "bridge"])
            .output();

        // Set bridge up
        Command::new("ip")
            .args(&["link", "set", &self.network_cfg.bridge_name, "up"])
            .output()?;

        // Add IP to bridge
        let _ = Command::new("ip")
            .args(&["addr", "add", &format!("{}/24", self.network_cfg.gateway),
                   "dev", &self.network_cfg.bridge_name])
            .output();

        // Enable IP forwarding
        fs::write("/proc/sys/net/ipv4/ip_forward", "1")?;

        // Setup NAT
        let _ = Command::new("iptables")
            .args(&["-t", "nat", "-A", "POSTROUTING",
                   "-s", &self.network_cfg.subnet,
                   "!", "-d", &self.network_cfg.subnet,
                   "-j", "MASQUERADE"])
            .output();

        info!("Network setup complete");
        Ok(())
    }

    /// Create a TAP device for a VM
    fn create_tap_device(&self, vm_id: &str) -> Result<String> {
        let tap_name = format!("fc-tap-{}", &vm_id[..8]);

        // Create TAP device
        Command::new("ip")
            .args(&["tuntap", "add", &tap_name, "mode", "tap"])
            .output()
            .context("Failed to create TAP device")?;

        // Set TAP up
        Command::new("ip")
            .args(&["link", "set", &tap_name, "up"])
            .output()?;

        // Add to bridge
        Command::new("ip")
            .args(&["link", "set", &tap_name, "master", &self.network_cfg.bridge_name])
            .output()?;

        Ok(tap_name)
    }

    /// Launch a new VM
    pub async fn launch_vm(&self, mut config: VmConfig) -> Result<String> {
        let vm_id = Uuid::new_v4().to_string();
        let vm_dir = self.base_dir.join(&vm_id);
        fs::create_dir_all(&vm_dir)?;

        info!("Launching Firecracker VM: {}", vm_id);

        // Verify kernel and rootfs exist
        if !config.kernel_path.exists() {
            return Err(anyhow::anyhow!("Kernel not found: {:?}", config.kernel_path));
        }
        if !config.rootfs_path.exists() {
            return Err(anyhow::anyhow!("Rootfs not found: {:?}", config.rootfs_path));
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

        // Create VM configuration file
        let vm_config_path = vm_dir.join("vm_config.json");
        let config_json = self.create_vm_config_json(&config)?;
        fs::write(&vm_config_path, config_json)?;

        // Launch Firecracker
        let mut cmd = if config.enable_jailer && self.jailer_bin.is_some() {
            self.create_jailer_command(&vm_id, &config, &api_socket)?
        } else {
            self.create_firecracker_command(&api_socket)?
        };

        let process = cmd.spawn()
            .context("Failed to spawn Firecracker process")?;

        // Wait for API socket
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Configure VM via API
        self.configure_vm_api(&api_socket, &config).await?;

        // Create VM instance
        let instance = VmInstance {
            id: vm_id.clone(),
            config,
            process: Some(process),
            api_socket,
            metrics: VmMetrics {
                start_time: std::time::Instant::now(),
                cpu_usage_ms: 0,
                memory_usage_bytes: 0,
                network_rx_bytes: 0,
                network_tx_bytes: 0,
            },
            state: VmState::Running,
        };

        // Store VM
        let mut vms = self.vms.write().await;
        vms.insert(vm_id.clone(), Arc::new(RwLock::new(instance)));

        info!("VM {} launched successfully", vm_id);
        Ok(vm_id)
    }

    fn create_firecracker_command(&self, api_socket: &Path) -> Result<Command> {
        let mut cmd = Command::new(&self.firecracker_bin);
        cmd.arg("--api-sock").arg(api_socket)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Ok(cmd)
    }

    fn create_jailer_command(&self, vm_id: &str, config: &VmConfig, api_socket: &Path) -> Result<Command> {
        let jailer_bin = self.jailer_bin.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Jailer not available"))?;

        let jailer_cfg = config.jailer_cfg.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Jailer config required"))?;

        let mut cmd = Command::new(jailer_bin);
        cmd.arg("--id").arg(&jailer_cfg.id)
            .arg("--exec-file").arg(&self.firecracker_bin)
            .arg("--uid").arg(jailer_cfg.uid.to_string())
            .arg("--gid").arg(jailer_cfg.gid.to_string())
            .arg("--chroot-base-dir").arg(&jailer_cfg.chroot_base_dir);

        if jailer_cfg.daemonize {
            cmd.arg("--daemonize");
        }

        cmd.arg("--")
            .arg("--api-sock").arg(api_socket);

        Ok(cmd)
    }

    fn create_vm_config_json(&self, config: &VmConfig) -> Result<String> {
        let config_obj = serde_json::json!({
            "boot-source": {
                "kernel_image_path": config.kernel_path,
                "boot_args": config.kernel_args
            },
            "drives": [{
                "drive_id": "rootfs",
                "path_on_host": config.rootfs_path,
                "is_root_device": true,
                "is_read_only": false
            }],
            "machine-config": {
                "vcpu_count": config.vcpu_count,
                "mem_size_mib": config.mem_size_mib,
                "smt": false,
                "track_dirty_pages": false
            },
            "network-interfaces": config.network_interfaces.iter().map(|iface| {
                serde_json::json!({
                    "iface_id": iface.iface_id,
                    "host_dev_name": iface.host_dev_name,
                    "guest_mac": iface.guest_mac
                })
            }).collect::<Vec<_>>()
        });

        Ok(serde_json::to_string_pretty(&config_obj)?)
    }

    async fn configure_vm_api(&self, api_socket: &Path, config: &VmConfig) -> Result<()> {
        // Wait for socket to be ready
        let max_retries = 10;
        for i in 0..max_retries {
            if api_socket.exists() {
                break;
            }
            if i == max_retries - 1 {
                return Err(anyhow::anyhow!("API socket not ready"));
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // Use curl to configure the VM via API
        // In production, you'd use an HTTP client library

        // Set boot source
        let boot_source = serde_json::json!({
            "kernel_image_path": config.kernel_path.to_str().unwrap(),
            "boot_args": config.kernel_args
        });

        self.api_request(api_socket, "PUT", "/boot-source", &boot_source).await?;

        // Set machine config
        let machine_config = serde_json::json!({
            "vcpu_count": config.vcpu_count,
            "mem_size_mib": config.mem_size_mib,
            "smt": false
        });

        self.api_request(api_socket, "PUT", "/machine-config", &machine_config).await?;

        // Add rootfs drive
        let drive = serde_json::json!({
            "drive_id": "rootfs",
            "path_on_host": config.rootfs_path.to_str().unwrap(),
            "is_root_device": true,
            "is_read_only": false
        });

        self.api_request(api_socket, "PUT", "/drives/rootfs", &drive).await?;

        // Configure network interfaces
        for iface in &config.network_interfaces {
            let net_config = serde_json::json!({
                "iface_id": iface.iface_id,
                "host_dev_name": iface.host_dev_name,
                "guest_mac": iface.guest_mac
            });

            self.api_request(api_socket, "PUT",
                            &format!("/network-interfaces/{}", iface.iface_id),
                            &net_config).await?;
        }

        // Start the VM
        let action = serde_json::json!({
            "action_type": "InstanceStart"
        });

        self.api_request(api_socket, "PUT", "/actions", &action).await?;

        Ok(())
    }

    async fn api_request(&self, socket: &Path, method: &str, path: &str, body: &serde_json::Value) -> Result<()> {
        let socket_path = socket.to_str().unwrap();
        let body_str = serde_json::to_string(body)?;

        let output = Command::new("curl")
            .arg("--unix-socket").arg(socket_path)
            .arg("-X").arg(method)
            .arg("-H").arg("Content-Type: application/json")
            .arg("-d").arg(body_str)
            .arg(format!("http://localhost{}", path))
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("API request failed: {}", stderr));
        }

        Ok(())
    }

    fn generate_mac() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        format!("02:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                rng.gen::<u8>(),
                rng.gen::<u8>(),
                rng.gen::<u8>(),
                rng.gen::<u8>(),
                rng.gen::<u8>())
    }

    /// Stop a VM
    pub async fn stop_vm(&self, vm_id: &str) -> Result<()> {
        let vms = self.vms.read().await;

        if let Some(vm_arc) = vms.get(vm_id) {
            let mut vm = vm_arc.write().await;

            // Send shutdown action via API
            let action = serde_json::json!({
                "action_type": "SendCtrlAltDel"
            });

            self.api_request(&vm.api_socket, "PUT", "/actions", &action).await?;

            // Wait a bit for graceful shutdown
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            // Force kill if still running
            if let Some(mut process) = vm.process.take() {
                let _ = process.kill();
                let _ = process.wait();
            }

            vm.state = VmState::Stopped;
            info!("VM {} stopped", vm_id);
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