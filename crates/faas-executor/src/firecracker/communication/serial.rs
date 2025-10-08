//! Serial Console Communication
//!
//! Fallback communication method using serial console for VMs

use super::{CommunicationError, Result};
use std::io::{BufRead, Write};
use std::time::Duration;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
use tracing::{error, info};

/// Serial console connection to a VM
pub struct SerialConsole {
    device_path: String,
    timeout: Duration,
}

impl SerialConsole {
    /// Create a new serial console connection
    pub fn new(device_path: String, timeout: Duration) -> Self {
        Self {
            device_path,
            timeout,
        }
    }

    /// Execute a command via serial console
    pub async fn execute_command(&self, command: &str, payload: &[u8]) -> Result<Vec<u8>> {
        info!("Executing command via serial console: {}", self.device_path);

        // Open serial device for read/write
        let mut device = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.device_path)
            .await
            .map_err(|e| {
                CommunicationError::ConnectionFailed(format!(
                    "Failed to open serial device {}: {}",
                    self.device_path, e
                ))
            })?;

        // Configure serial port (if it's a real serial device)
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = device.as_raw_fd();

            unsafe {
                // Set terminal attributes for raw mode
                let mut termios: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(fd, &mut termios) == 0 {
                    // Raw mode
                    termios.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
                    termios.c_iflag &= !(libc::IXON | libc::ICRNL);
                    termios.c_oflag &= !libc::OPOST;

                    // 8N1
                    termios.c_cflag &= !libc::CSIZE;
                    termios.c_cflag |= libc::CS8;
                    termios.c_cflag &= !(libc::PARENB | libc::CSTOPB);

                    // Set baud rate to 115200
                    libc::cfsetispeed(&mut termios, libc::B115200);
                    libc::cfsetospeed(&mut termios, libc::B115200);

                    libc::tcsetattr(fd, libc::TCSANOW, &termios);
                }
            }
        }

        // Create a unique marker for command boundaries
        let start_marker = format!("<<<FAAS_START_{}>>>", uuid::Uuid::new_v4());
        let end_marker = format!("<<<FAAS_END_{}>>>", uuid::Uuid::new_v4());

        // Send command with markers
        let full_command = if !payload.is_empty() {
            // If we have payload, pipe it to the command
            format!(
                "echo '{}'; echo '{}' | {} 2>&1; EXIT_CODE=$?; echo '{}'; echo $EXIT_CODE\n",
                start_marker,
                base64::encode(payload),
                command,
                end_marker
            )
        } else {
            format!(
                "echo '{start_marker}'; {command} 2>&1; EXIT_CODE=$?; echo '{end_marker}'; echo $EXIT_CODE\n"
            )
        };

        // Write command
        device.write_all(full_command.as_bytes()).await?;
        device.flush().await?;

        // Read response with timeout
        let output = tokio::time::timeout(self.timeout, async {
            let mut reader = AsyncBufReader::new(device);
            let mut output = Vec::new();
            let mut capturing = false;
            let mut exit_code = 0;

            loop {
                let mut line = String::new();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();

                        if trimmed == start_marker {
                            capturing = true;
                            continue;
                        }

                        if trimmed == end_marker {
                            // Next line should be exit code
                            let mut exit_line = String::new();
                            if reader.read_line(&mut exit_line).await.is_ok() {
                                exit_code = exit_line.trim().parse().unwrap_or(0);
                            }
                            break;
                        }

                        if capturing {
                            output.extend_from_slice(line.as_bytes());
                        }
                    }
                    Err(e) => {
                        error!("Error reading from serial: {}", e);
                        break;
                    }
                }
            }

            if exit_code != 0 {
                Err(CommunicationError::ExecutionFailed(format!(
                    "Command failed with exit code {exit_code}"
                )))
            } else {
                Ok(output)
            }
        })
        .await
        .map_err(|_| CommunicationError::Timeout)??;

        Ok(output)
    }

    /// Send input to the serial console
    pub async fn send_input(&self, data: &[u8]) -> Result<()> {
        let mut device = OpenOptions::new()
            .write(true)
            .open(&self.device_path)
            .await?;

        device.write_all(data).await?;
        device.flush().await?;

        Ok(())
    }

    /// Read output from the serial console
    pub async fn read_output(&self, max_bytes: usize) -> Result<Vec<u8>> {
        let device = OpenOptions::new()
            .read(true)
            .open(&self.device_path)
            .await?;

        let mut buffer = vec![0u8; max_bytes];
        let mut reader = AsyncBufReader::new(device);

        let bytes_read = tokio::time::timeout(self.timeout, async {
            let mut total = 0;
            while total < max_bytes {
                match reader.read(&mut buffer[total..]).await {
                    Ok(0) => break,
                    Ok(n) => total += n,
                    Err(e) => return Err(e),
                }
            }
            Ok(total)
        })
        .await
        .map_err(|_| CommunicationError::Timeout)?
        .map_err(|e: std::io::Error| CommunicationError::Io(e))?;

        buffer.truncate(bytes_read);
        Ok(buffer)
    }

    /// Wait for a specific prompt or pattern in the output
    pub async fn wait_for_prompt(&self, prompt: &str) -> Result<()> {
        let start = std::time::Instant::now();
        let device = OpenOptions::new()
            .read(true)
            .open(&self.device_path)
            .await?;

        let mut reader = AsyncBufReader::new(device);
        let mut buffer = String::new();

        while start.elapsed() < self.timeout {
            let mut line = String::new();
            match tokio::time::timeout(Duration::from_millis(100), reader.read_line(&mut line)).await
            {
                Ok(Ok(0)) => continue, // EOF
                Ok(Ok(_)) => {
                    buffer.push_str(&line);
                    if buffer.contains(prompt) {
                        return Ok(());
                    }
                    // Keep only last 1KB to avoid unbounded growth
                    if buffer.len() > 1024 {
                        buffer.drain(..buffer.len() - 1024);
                    }
                }
                _ => continue,
            }
        }

        Err(CommunicationError::Timeout)
    }
}

use tokio::io::AsyncReadExt;