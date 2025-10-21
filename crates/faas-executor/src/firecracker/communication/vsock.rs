//! Vsock Communication Implementation
//!
//! Uses virtio-vsock for high-performance VM communication

use super::{CommunicationError, Result};
#[cfg(target_os = "linux")]
use std::io::{Read, Write};
use std::time::Duration;
use tracing::info;

/// Vsock connection to a Firecracker VM
pub struct VsockConnection {
    cid: u32,
    port: u32,
    timeout: Duration,
}

impl VsockConnection {
    /// Create a new vsock connection
    pub fn new(cid: u32, port: u32, timeout: Duration) -> Self {
        Self { cid, port, timeout }
    }

    /// Execute a command in the VM via vsock
    pub async fn execute_command(&self, command: &str, payload: &[u8]) -> Result<Vec<u8>> {
        info!(
            "Executing command via vsock: CID={}, port={}",
            self.cid, self.port
        );

        // On Linux, connect to vsock
        #[cfg(target_os = "linux")]
        {
            use libc::{sockaddr_vm, AF_VSOCK, SOCK_STREAM};
            use std::mem;
            use std::os::unix::io::FromRawFd;

            unsafe {
                // Create vsock socket
                let sock_fd = libc::socket(AF_VSOCK, SOCK_STREAM, 0);
                if sock_fd < 0 {
                    return Err(CommunicationError::ConnectionFailed(
                        "Failed to create vsock socket".to_string(),
                    ));
                }

                // Prepare address
                let mut addr: sockaddr_vm = mem::zeroed();
                addr.svm_family = AF_VSOCK as u16;
                addr.svm_cid = self.cid;
                addr.svm_port = self.port;

                // Connect with timeout
                let addr_ptr = &addr as *const sockaddr_vm as *const libc::sockaddr;
                let addr_len = mem::size_of::<sockaddr_vm>() as libc::socklen_t;

                // Set socket to non-blocking for timeout handling
                let flags = libc::fcntl(sock_fd, libc::F_GETFL, 0);
                libc::fcntl(sock_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);

                let connect_result = libc::connect(sock_fd, addr_ptr, addr_len);

                if connect_result < 0 {
                    let err = std::io::Error::last_os_error();
                    if err.raw_os_error() != Some(libc::EINPROGRESS) {
                        libc::close(sock_fd);
                        return Err(CommunicationError::ConnectionFailed(format!(
                            "Failed to connect to vsock: {}",
                            err
                        )));
                    }

                    // Wait for connection with select
                    let mut write_fds: libc::fd_set = mem::zeroed();
                    libc::FD_SET(sock_fd, &mut write_fds);

                    let mut tv = libc::timeval {
                        tv_sec: self.timeout.as_secs() as i64,
                        tv_usec: 0,
                    };

                    let select_result = libc::select(
                        sock_fd + 1,
                        std::ptr::null_mut(),
                        &mut write_fds,
                        std::ptr::null_mut(),
                        &mut tv,
                    );

                    if select_result <= 0 {
                        libc::close(sock_fd);
                        return Err(CommunicationError::Timeout);
                    }
                }

                // Reset to blocking mode
                libc::fcntl(sock_fd, libc::F_SETFL, flags);

                // Create stream from raw fd
                let mut stream = std::fs::File::from_raw_fd(sock_fd);

                // Send command
                let message = VsockMessage {
                    command: command.to_string(),
                    payload: payload.to_vec(),
                };

                let serialized = serde_json::to_vec(&message)
                    .map_err(|e| CommunicationError::ExecutionFailed(e.to_string()))?;

                // Write length prefix
                let len = serialized.len() as u32;
                stream.write_all(&len.to_le_bytes())?;
                stream.write_all(&serialized)?;
                stream.flush()?;

                // Read response length
                let mut len_bytes = [0u8; 4];
                stream.read_exact(&mut len_bytes)?;
                let response_len = u32::from_le_bytes(len_bytes) as usize;

                // Read response
                let mut response = vec![0u8; response_len];
                stream.read_exact(&mut response)?;

                // Parse response
                let result: VsockResponse = serde_json::from_slice(&response)
                    .map_err(|e| CommunicationError::ExecutionFailed(e.to_string()))?;

                if result.success {
                    Ok(result.output)
                } else {
                    Err(CommunicationError::ExecutionFailed(
                        result.error.unwrap_or_else(|| "Unknown error".to_string()),
                    ))
                }
            }
        }

        // Vsock not available on non-Linux
        #[cfg(not(target_os = "linux"))]
        {
            Err(CommunicationError::VsockUnavailable)
        }
    }

    /// Check if vsock is available
    pub fn is_available() -> bool {
        #[cfg(target_os = "linux")]
        {
            // Check if vsock module is loaded
            std::path::Path::new("/dev/vsock").exists()
        }

        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct VsockMessage {
    command: String,
    payload: Vec<u8>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct VsockResponse {
    success: bool,
    output: Vec<u8>,
    error: Option<String>,
    exit_code: i32,
}

/// Vsock server that runs inside the VM to handle commands
pub struct VsockServer {
    port: u32,
}

impl VsockServer {
    pub fn new(port: u32) -> Self {
        Self { port }
    }

    /// Start the vsock server inside the VM
    pub async fn start(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            use libc::{sockaddr_vm, AF_VSOCK, SOCK_STREAM, VMADDR_CID_ANY};
            use std::mem;
            use std::os::unix::io::FromRawFd;

            unsafe {
                // Create listening socket
                let sock_fd = libc::socket(AF_VSOCK, SOCK_STREAM, 0);
                if sock_fd < 0 {
                    return Err(CommunicationError::ConnectionFailed(
                        "Failed to create vsock socket".to_string(),
                    ));
                }

                // Set SO_REUSEADDR
                let reuse = 1i32;
                libc::setsockopt(
                    sock_fd,
                    libc::SOL_SOCKET,
                    libc::SO_REUSEADDR,
                    &reuse as *const _ as *const libc::c_void,
                    mem::size_of_val(&reuse) as u32,
                );

                // Bind to port
                let mut addr: sockaddr_vm = mem::zeroed();
                addr.svm_family = AF_VSOCK as u16;
                addr.svm_cid = VMADDR_CID_ANY;
                addr.svm_port = self.port;

                let addr_ptr = &addr as *const sockaddr_vm as *const libc::sockaddr;
                let addr_len = mem::size_of::<sockaddr_vm>() as libc::socklen_t;

                if libc::bind(sock_fd, addr_ptr, addr_len) < 0 {
                    libc::close(sock_fd);
                    return Err(CommunicationError::ConnectionFailed(format!(
                        "Failed to bind vsock: {}",
                        std::io::Error::last_os_error()
                    )));
                }

                // Listen
                if libc::listen(sock_fd, 10) < 0 {
                    libc::close(sock_fd);
                    return Err(CommunicationError::ConnectionFailed(
                        "Failed to listen on vsock".to_string(),
                    ));
                }

                info!("Vsock server listening on port {}", self.port);

                // Accept loop
                loop {
                    let client_fd =
                        libc::accept(sock_fd, std::ptr::null_mut(), std::ptr::null_mut());
                    if client_fd < 0 {
                        continue;
                    }

                    // Handle client in a task
                    let client_stream = std::fs::File::from_raw_fd(client_fd);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(client_stream).await {
                            error!("Error handling vsock client: {}", e);
                        }
                    });
                }
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(CommunicationError::VsockUnavailable)
        }
    }

    #[cfg(target_os = "linux")]
    async fn handle_client(mut stream: std::fs::File) -> Result<()> {
        // Read message length
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes)?;
        let msg_len = u32::from_le_bytes(len_bytes) as usize;

        // Read message
        let mut msg_bytes = vec![0u8; msg_len];
        stream.read_exact(&mut msg_bytes)?;

        // Parse message
        let message: VsockMessage = serde_json::from_slice(&msg_bytes)
            .map_err(|e| CommunicationError::ExecutionFailed(e.to_string()))?;

        // Execute command
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&message.command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()?;

        // Prepare response
        let response = VsockResponse {
            success: output.status.success(),
            output: output.stdout,
            error: if output.status.success() {
                None
            } else {
                Some(String::from_utf8_lossy(&output.stderr).to_string())
            },
            exit_code: output.status.code().unwrap_or(-1),
        };

        // Send response
        let serialized = serde_json::to_vec(&response)
            .map_err(|e| CommunicationError::ExecutionFailed(e.to_string()))?;

        let len = serialized.len() as u32;
        stream.write_all(&len.to_le_bytes())?;
        stream.write_all(&serialized)?;
        stream.flush()?;

        Ok(())
    }
}
