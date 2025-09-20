// --- FirecrackerExecutor Implementation (Placeholder) ---


use async_trait::async_trait;
use faas_common::{
    FaasError, InvocationResult, Result as CommonResult, SandboxConfig, SandboxExecutor,
};
use firecracker_rs_sdk::{
    firecracker::FirecrackerOption,
    models::{
        vsock::Vsock, BootSource, Drive, LogLevel, Logger, MachineConfiguration,
    },
    Error as FcError,
};
use serde_json;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempfile::Builder as TempFileBuilder;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{timeout, Duration};
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

// Default paths - These should be overridden via configuration
const DEFAULT_FIRECRACKER_BINARY_PATH: &str = "/usr/bin/firecracker";
const DEFAULT_KERNEL_IMAGE_PATH: &str = "resources/kernel/hello-vmlinux.bin";
const DEFAULT_ROOTFS_PATH: &str = "tools/firecracker-rootfs-builder/output/images/rootfs.ext2";
const LOG_FIFO_PATH_PREFIX: &str = "/tmp/fc_log_";
const API_SOCK_PATH_PREFIX: &str = "/tmp/fc_sock_";
const GUEST_VSOCK_PORT: u32 = 1234; // Port for guest agent vsock communication
const TEMP_ROOTFS_DIR_PREFIX: &str = "/tmp/faas_rootfs_"; // Directory for temp rootfs copies

// RAII Guard for Firecracker instance cleanup
struct InstanceGuard {
    instance_id: String,
    api_sock_path: String,
    log_fifo_path: String,
    vsock_uds_path: String,
    temp_rootfs_path: Option<PathBuf>, // Add path for temporary rootfs copy
    needs_cleanup: bool,
}

impl InstanceGuard {
    fn new(
        instance_id: String,
        api_sock_path: String,
        log_fifo_path: String,
        vsock_uds_path: String,
    ) -> Self {
        Self {
            instance_id,
            api_sock_path,
            log_fifo_path,
            vsock_uds_path,
            temp_rootfs_path: None, // Initialize as None
            needs_cleanup: true,
        }
    }

    // Method to set the temp path after creation
    fn set_temp_rootfs_path(&mut self, path: PathBuf) {
        self.temp_rootfs_path = Some(path);
    }

    async fn cleanup(&mut self) {
        if !self.needs_cleanup {
            return;
        }
        info!(instance_id=%self.instance_id, "Cleaning up Firecracker resources");
        // Remove files
        let _ = tokio::fs::remove_file(&self.api_sock_path)
            .await
            .map_err(|e| error!(error=%e, path=%self.api_sock_path, "Failed to remove API socket"));
        let _ = tokio::fs::remove_file(&self.log_fifo_path)
            .await
            .map_err(|e| error!(error=%e, path=%self.log_fifo_path, "Failed to remove log FIFO"));
        let _ = tokio::fs::remove_file(&self.vsock_uds_path)
            .await
            .map_err(|e| error!(error=%e, path=%self.vsock_uds_path, "Failed to remove vsock UDS"));

        // Remove temporary rootfs copy
        if let Some(path) = &self.temp_rootfs_path {
            let _ = tokio::fs::remove_file(path)
                .await
                .map_err(|e| error!(error=%e, path=?path, "Failed to remove temp rootfs copy"));
        }

        self.needs_cleanup = false;
    }
}

// Implement AsyncDrop if needed, otherwise call cleanup explicitly
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if self.needs_cleanup {
            warn!(instance_id=%self.instance_id, "InstanceGuard dropped without explicit async cleanup! Resources might leak.");
        }
    }
}

#[derive(Clone)]
pub struct FirecrackerExecutor {
    fc_binary_path: String,
    kernel_image_path: String,
}

impl FirecrackerExecutor {
    pub fn stub() -> Self {
        Self {
            fc_binary_path: String::new(),
            kernel_image_path: String::new(),
        }
    }

    pub fn new(fc_binary_path: String, kernel_image_path: String) -> Result<Self, FaasError> {
        info!(%fc_binary_path, %kernel_image_path, "Creating FirecrackerExecutor (async)");
        // Basic validation
        if !Path::new(&fc_binary_path).exists() {
            return Err(FaasError::Config(format!(
                "Firecracker binary not found at: {}",
                fc_binary_path
            )));
        }
        if !Path::new(&kernel_image_path).exists() {
            return Err(FaasError::Config(format!(
                "Kernel image not found at: {}",
                kernel_image_path
            )));
        }
        Ok(Self {
            fc_binary_path,
            kernel_image_path,
        })
    }
}

#[async_trait]
impl SandboxExecutor for FirecrackerExecutor {
    #[instrument(skip(self, config), fields(function_id = %config.function_id, source = %config.source))]
    async fn execute(&self, config: SandboxConfig) -> CommonResult<InvocationResult> {
        let instance_id = Uuid::new_v4().to_string();
        let api_sock_path = format!("{}{}.socket", API_SOCK_PATH_PREFIX, instance_id);
        let log_fifo_path = format!("{}{}.fifo", LOG_FIFO_PATH_PREFIX, instance_id);
        let base_rootfs_path = PathBuf::from(&config.source);
        if !base_rootfs_path.exists() {
            return Err(FaasError::Executor(format!(
                "Base rootfs not found at: {}",
                base_rootfs_path.display()
            )));
        }
        let vsock_uds_path = format!("/tmp/fc_vsock_{}.sock", instance_id);
        let request_id = instance_id.clone();

        // Prepare Instance Rootfs (Copy)
        let temp_rootfs_dir = TempFileBuilder::new()
            .prefix(&format!("{}{}_", TEMP_ROOTFS_DIR_PREFIX, instance_id))
            .tempdir()
            .map_err(|e| {
                FaasError::Executor(format!("Failed to create temp dir for rootfs: {}", e))
            })?;
        let rootfs_filename = base_rootfs_path
            .file_name()
            .unwrap_or(OsStr::new("rootfs.ext4"));
        let instance_rootfs_path = temp_rootfs_dir.path().join(rootfs_filename);
        info!(source=%base_rootfs_path.display(), dest=%instance_rootfs_path.display(), "Copying base rootfs for instance");
        tokio::fs::copy(&base_rootfs_path, &instance_rootfs_path)
            .await
            .map_err(|e| FaasError::Executor(format!("Failed to copy rootfs: {}", e)))?;

        // Guard setup
        let mut guard = InstanceGuard::new(
            instance_id.clone(),
            api_sock_path.clone(),
            log_fifo_path.clone(),
            vsock_uds_path.clone(),
        );
        guard.set_temp_rootfs_path(instance_rootfs_path.clone()); // Pass the *copied* path

        info!(%instance_id, %api_sock_path, %log_fifo_path, rootfs_path=%instance_rootfs_path.display(), "Preparing Firecracker execution (async)");

        let result = async {
            let mut instance = FirecrackerOption::new(&self.fc_binary_path)
                .id(&instance_id)
                .api_sock(&api_sock_path)
                .build()?;

            instance.put_logger(&Logger {
                 log_path: log_fifo_path.clone().into(),
                 level: Some(LogLevel::Info),
                 show_level: Some(false),
                 show_log_origin: Some(false),
                 module: None,
             }).await?;

            instance.start_vmm().await?;

            instance.put_machine_configuration(&MachineConfiguration {
                 vcpu_count: 1,
                 mem_size_mib: 1024,
                 smt: Some(false),
                 cpu_template: None,
                 track_dirty_pages: None,
                 huge_pages: None,
             }).await?;

            let boot_args = format!(
                "console=ttyS0 reboot=k panic=1 pci=off quiet loglevel=0 vsock_cid=3"
            );
            instance.put_guest_boot_source(&BootSource {
                 kernel_image_path: self.kernel_image_path.clone().into(),
                 boot_args: Some(boot_args),
                 initrd_path: None,
             }).await?;

            instance.put_guest_drive_by_id(&Drive {
                 drive_id: "rootfs".into(),
                 path_on_host: instance_rootfs_path, // Use the copied path
                 is_root_device: true,
                 is_read_only: false,
                 cache_type: None,
                 partuuid: None,
                 rate_limiter: None,
                 io_engine: None,
                 socket: None,
             }).await?;

            instance.put_guest_vsock(&Vsock {
                 vsock_id: Some("vsock0".to_string()),
                 guest_cid: 3,
                 uds_path: vsock_uds_path.clone().into(),
             }).await?;

            instance.start().await?;
            info!(%instance_id, "VM started. Attempting vsock connection...");

            let connect_timeout = Duration::from_secs(5);
            let stream_result = timeout(connect_timeout, async {
                 loop {
                     match UnixStream::connect(&vsock_uds_path).await {
                         Ok(s) => break Ok(s),
                         Err(e) => {
                             warn!(error=%e, path=%vsock_uds_path, "Failed vsock connect attempt, retrying...");
                             tokio::time::sleep(Duration::from_millis(100)).await;
                         }
                     }
                 }
             }).await;

            let stream = match stream_result {
                Ok(Ok(s)) => {
                    info!(%instance_id, "Vsock UDS connected.");
                    s
                },
                Ok(Err(e)) => {
                     error!(error=%e, "Vsock connect loop internal error");
                     return Err(FcError::IO(e));
                },
                Err(_) => {
                     error!(timeout=?connect_timeout, path=%vsock_uds_path, "Timeout connecting to vsock UDS");
                     return Err(FcError::Configuration("Timeout connecting to guest vsock".to_string()));
                }
            };
            let (mut reader, mut writer) = stream.into_split();

            info!(%instance_id, "Sending config...");
            let config_json = serde_json::to_vec(&config)
                 .map_err(|e| FcError::Configuration(format!("Config serialization failed: {}", e)))?;
            match timeout(Duration::from_secs(5), writer.write_all(&config_json)).await {
                Ok(Ok(())) => info!(%instance_id, "Config sent."),
                Ok(Err(e)) => return Err(FcError::IO(e)),
                Err(_) => return Err(FcError::Configuration("Timeout sending config via vsock".into())),
            }
             match timeout(Duration::from_secs(1), writer.shutdown()).await {
                 Ok(Ok(())) => {}, // Shutdown successful
                 Ok(Err(e)) => warn!(error=%e, "Error shutting down vsock writer"),
                 Err(_) => warn!("Timeout shutting down vsock writer"),
             }

            info!(%instance_id, "Reading response from vsock...");
            let mut buffer = Vec::new();
            // Timeout for reading the response from the guest
            let read_timeout_duration = Duration::from_secs(60); // TODO: Make this configurable
            match timeout(read_timeout_duration, reader.read_to_end(&mut buffer)).await {
                Ok(Ok(_)) => {
                    // Successfully read some bytes (or 0 if EOF)
                    info!(%instance_id, bytes_read=buffer.len(), "Successfully read response from vsock");
                }
                Ok(Err(e)) => {
                 error!(%instance_id, error=%e, "Vsock read error");
                 return Err(FcError::IO(e));
                }
                Err(_) => {
                    error!(%instance_id, timeout=?read_timeout_duration, "Timeout reading response from vsock");
                    return Err(FcError::Instance("Timeout reading response from guest vsock".to_string()));
                }
            }

            let invocation_result: InvocationResult = serde_json::from_slice(&buffer)
                 .map_err(|e| FcError::Configuration(format!("Result deserialization failed: {}", e)))?;

            info!(%instance_id, request_id=%invocation_result.request_id, "Received invocation result from guest");

            info!(%instance_id, "Stopping VM...");
            match timeout(Duration::from_secs(5), instance.stop()).await {
                Ok(Ok(())) => info!(%instance_id, "VM stop command successful."),
                Ok(Err(e)) => return Err(e),
                Err(_) => return Err(FcError::Instance("Timeout stopping VM".into())),
            }

            Ok::<_, FcError>(invocation_result)
        }.await;

        guard.cleanup().await;

        match result {
            Ok(invocation_result) => Ok(invocation_result),
            Err(fc_err) => {
                error!(error = %fc_err, "Firecracker SDK operation failed");
                let reason = match fc_err {
                    FcError::IO(e) => format!("IO Error: {}", e),
                    FcError::Agent(e) => format!("Agent Error: {}", e),
                    FcError::Configuration(e) => format!("Configuration Error: {}", e),
                    FcError::Event(e) => format!("Event Error: {}", e),
                    FcError::Instance(e) => format!("Instance Error: {}", e),
                    FcError::FeatureNone(e) => format!("Feature Error: {}", e),
                };
                Err(FaasError::Executor(format!(
                    "Firecracker error: {}",
                    reason
                )))
            }
        }
    }
}
