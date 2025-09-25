#!/usr/bin/env rust-script
//! Guest agent that runs inside Firecracker VMs to handle command execution
//!
//! This should be compiled and included in the rootfs image
//! ```cargo
//! [dependencies]
//! serde = { version = "1.0", features = ["derive"] }
//! serde_json = "1.0"
//! libc = "0.2"
//! ```

use std::io::{Read, Write};
use std::os::unix::io::FromRawFd;
use std::process::{Command, Stdio};

const VSOCK_PORT: u32 = 5555;

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

fn main() {
    println!("FaaS Guest Agent starting...");

    // Try to set up vsock server
    if let Err(e) = run_vsock_server() {
        eprintln!("Failed to run vsock server: {}", e);
        // Fall back to serial console mode
        run_serial_console();
    }
}

fn run_vsock_server() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    {
        use libc::{sockaddr_vm, AF_VSOCK, SOCK_STREAM, VMADDR_CID_ANY};
        use std::mem;

        unsafe {
            // Create vsock listening socket
            let sock_fd = libc::socket(AF_VSOCK, SOCK_STREAM, 0);
            if sock_fd < 0 {
                return Err("Failed to create vsock socket".into());
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
            addr.svm_port = VSOCK_PORT;

            let addr_ptr = &addr as *const sockaddr_vm as *const libc::sockaddr;
            let addr_len = mem::size_of::<sockaddr_vm>() as libc::socklen_t;

            if libc::bind(sock_fd, addr_ptr, addr_len) < 0 {
                libc::close(sock_fd);
                return Err("Failed to bind vsock".into());
            }

            // Listen
            if libc::listen(sock_fd, 10) < 0 {
                libc::close(sock_fd);
                return Err("Failed to listen on vsock".into());
            }

            println!("Guest agent listening on vsock port {}", VSOCK_PORT);

            // Accept connections
            loop {
                let client_fd = libc::accept(sock_fd, std::ptr::null_mut(), std::ptr::null_mut());
                if client_fd < 0 {
                    continue;
                }

                // Handle client
                let mut client = std::fs::File::from_raw_fd(client_fd);
                if let Err(e) = handle_vsock_client(&mut client) {
                    eprintln!("Error handling client: {}", e);
                }
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err("Vsock only available on Linux".into())
    }
}

fn handle_vsock_client(stream: &mut std::fs::File) -> Result<(), Box<dyn std::error::Error>> {
    // Read message length
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let msg_len = u32::from_le_bytes(len_bytes) as usize;

    // Read message
    let mut msg_bytes = vec![0u8; msg_len];
    stream.read_exact(&mut msg_bytes)?;

    // Parse message
    let message: VsockMessage = serde_json::from_slice(&msg_bytes)?;

    // Execute command
    let mut cmd = Command::new("sh")
        .arg("-c")
        .arg(&message.command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Write payload to stdin if present
    if !message.payload.is_empty() {
        if let Some(mut stdin) = cmd.stdin.take() {
            stdin.write_all(&message.payload)?;
        }
    }

    // Wait for completion
    let output = cmd.wait_with_output()?;

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
    let serialized = serde_json::to_vec(&response)?;
    let len = serialized.len() as u32;
    stream.write_all(&len.to_le_bytes())?;
    stream.write_all(&serialized)?;
    stream.flush()?;

    Ok(())
}

fn run_serial_console() {
    println!("Falling back to serial console mode");

    loop {
        // Read commands from serial console
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err() {
            continue;
        }

        let input = input.trim();

        // Look for our command markers
        if input.starts_with("<<<FAAS_START_") {
            if let Some(end_pos) = input.find(">>>") {
                let marker = &input[..end_pos + 3];
                let end_marker = marker.replace("START", "END");

                // Read the actual command
                let mut command = String::new();
                if std::io::stdin().read_line(&mut command).is_ok() {
                    // Execute command
                    let output = Command::new("sh")
                        .arg("-c")
                        .arg(command.trim())
                        .output()
                        .unwrap_or_else(|e| {
                            std::process::Output {
                                status: std::process::ExitStatus::from_raw(1),
                                stdout: Vec::new(),
                                stderr: format!("Failed to execute: {}", e).into_bytes(),
                            }
                        });

                    // Output the result
                    println!("{}", marker);
                    print!("{}", String::from_utf8_lossy(&output.stdout));
                    if !output.stderr.is_empty() {
                        eprint!("{}", String::from_utf8_lossy(&output.stderr));
                    }
                    println!("{}", end_marker);
                    println!("{}", output.status.code().unwrap_or(1));
                }
            }
        }
    }
}