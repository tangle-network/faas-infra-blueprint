//! VM Command Executor
//!
//! Unified interface for executing commands in VMs using the best available method

use super::{
    CommunicationConfig, CommunicationError, Result, SerialConsole, VsockConnection,
};
use faas_common::SandboxConfig;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Executes commands in VMs using the best available communication method
pub struct VmCommandExecutor {
    config: CommunicationConfig,
}

impl VmCommandExecutor {
    /// Create a new VM command executor
    pub fn new(config: CommunicationConfig) -> Self {
        Self { config }
    }

    /// Execute a command in the VM using the best available method
    pub async fn execute(&self, sandbox_config: &SandboxConfig) -> Result<Vec<u8>> {
        let command = sandbox_config.command.join(" ");
        let payload = &sandbox_config.payload;

        // Try methods in order of preference:
        // 1. Vsock (fastest, most reliable)
        // 2. SSH (secure, network-based)
        // 3. Serial console (fallback)

        // Try vsock first if available
        if let Some(cid) = self.config.vsock_cid {
            if VsockConnection::is_available() {
                info!("Using vsock for VM communication (CID: {})", cid);
                match self.execute_via_vsock(cid, &command, payload).await {
                    Ok(output) => return Ok(output),
                    Err(e) => {
                        warn!("Vsock execution failed, trying next method: {}", e);
                    }
                }
            }
        }

        // Try SSH if configured
        if let Some(ref ssh_config) = self.config.ssh_config {
            info!("Using SSH for VM communication");
            match self.execute_via_ssh(ssh_config, &command, payload).await {
                Ok(output) => return Ok(output),
                Err(e) => {
                    warn!("SSH execution failed, trying next method: {}", e);
                }
            }
        }

        // Fall back to serial console
        if let Some(ref serial_device) = self.config.serial_device {
            info!("Using serial console for VM communication");
            return self.execute_via_serial(serial_device, &command, payload).await;
        }

        Err(CommunicationError::ConnectionFailed(
            "No communication method available".to_string(),
        ))
    }

    /// Execute via vsock
    async fn execute_via_vsock(&self, cid: u32, command: &str, payload: &[u8]) -> Result<Vec<u8>> {
        let vsock = VsockConnection::new(
            cid,
            5555, // Default vsock port for command execution
            self.config.timeout,
        );

        // Retry logic
        let mut last_error = None;
        for attempt in 1..=self.config.retry_attempts {
            debug!("Vsock execution attempt {}/{}", attempt, self.config.retry_attempts);

            match vsock.execute_command(command, payload).await {
                Ok(output) => return Ok(output),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.config.retry_attempts {
                        tokio::time::sleep(self.config.retry_delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// Execute via SSH
    async fn execute_via_ssh(
        &self,
        ssh_config: &super::SshConfig,
        command: &str,
        payload: &[u8],
    ) -> Result<Vec<u8>> {
        // Use SSH to execute command
        // This would integrate with our existing SSH implementation

        use std::process::Stdio;
        use tokio::process::Command;

        let ssh_command = if let Some(ref key_path) = ssh_config.private_key_path {
            format!(
                "ssh -i {} -p {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null {}@{} '{}'",
                key_path, ssh_config.port, ssh_config.username, ssh_config.host, command
            )
        } else {
            format!(
                "ssh -p {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null {}@{} '{}'",
                ssh_config.port, ssh_config.username, ssh_config.host, command
            )
        };

        let mut process = Command::new("sh")
            .arg("-c")
            .arg(&ssh_command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CommunicationError::ExecutionFailed(e.to_string()))?;

        // Write payload to stdin if present
        if !payload.is_empty() {
            if let Some(mut stdin) = process.stdin.take() {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(payload).await?;
                stdin.flush().await?;
            }
        }

        // Wait for completion with timeout
        let output = tokio::time::timeout(self.config.timeout, process.wait_with_output())
            .await
            .map_err(|_| CommunicationError::Timeout)?
            .map_err(|e| CommunicationError::ExecutionFailed(e.to_string()))?;

        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(CommunicationError::ExecutionFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    /// Execute via serial console
    async fn execute_via_serial(
        &self,
        serial_device: &str,
        command: &str,
        payload: &[u8],
    ) -> Result<Vec<u8>> {
        let serial = SerialConsole::new(serial_device.to_string(), self.config.timeout);

        // Retry logic for serial (it's less reliable)
        let mut last_error = None;
        for attempt in 1..=self.config.retry_attempts {
            debug!("Serial execution attempt {}/{}", attempt, self.config.retry_attempts);

            match serial.execute_command(command, payload).await {
                Ok(output) => return Ok(output),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.config.retry_attempts {
                        // Longer delay for serial retries
                        tokio::time::sleep(self.config.retry_delay * 2).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// Test connectivity to the VM
    pub async fn test_connection(&self) -> Result<()> {
        // Try a simple echo command
        let test_config = SandboxConfig {
            function_id: "test".to_string(),
            source: "test".to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            env_vars: None,
            payload: vec![],
            runtime: Some(faas_common::Runtime::Firecracker),
            execution_mode: None,
            memory_limit: None,
            timeout: Some(5000),  // 5 second timeout for test
        };

        match self.execute(&test_config).await {
            Ok(output) => {
                if output == b"test\n" || output == b"test" {
                    Ok(())
                } else {
                    Err(CommunicationError::ConnectionFailed(
                        "Test command returned unexpected output".to_string(),
                    ))
                }
            }
            Err(e) => Err(e),
        }
    }
}