//! VM Communication Module
//!
//! Provides production-ready communication with Firecracker VMs via:
//! - Serial console for basic I/O
//! - Vsock for high-performance bidirectional communication
//! - SSH for secure remote execution

pub mod serial;
pub mod vsock;
pub mod executor;

pub use executor::VmCommandExecutor;
pub use serial::SerialConsole;
pub use vsock::VsockConnection;

use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommunicationError {
    #[error("Failed to connect to VM: {0}")]
    ConnectionFailed(String),

    #[error("Command execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Timeout waiting for VM response")]
    Timeout,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Vsock not available")]
    VsockUnavailable,

    #[error("Serial console not available")]
    SerialUnavailable,
}

pub type Result<T> = std::result::Result<T, CommunicationError>;

/// VM communication configuration
#[derive(Debug, Clone)]
pub struct CommunicationConfig {
    /// Vsock CID for the VM (Context ID)
    pub vsock_cid: Option<u32>,

    /// Serial device path
    pub serial_device: Option<String>,

    /// SSH configuration if available
    pub ssh_config: Option<SshConfig>,

    /// Command execution timeout
    pub timeout: Duration,

    /// Retry configuration
    pub retry_attempts: u32,
    pub retry_delay: Duration,
}

#[derive(Debug, Clone)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub private_key_path: Option<String>,
}

impl Default for CommunicationConfig {
    fn default() -> Self {
        Self {
            vsock_cid: None,
            serial_device: None,
            ssh_config: None,
            timeout: Duration::from_secs(30),
            retry_attempts: 3,
            retry_delay: Duration::from_millis(100),
        }
    }
}