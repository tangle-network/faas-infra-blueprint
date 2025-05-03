use faas_common::{InvocationResult, SandboxConfig};
use serde_json;
use std::net::Shutdown;
use std::process::Stdio;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Command;
use tokio::time::Duration;
use tokio_vsock::{VsockListener, VsockStream};
use tracing::{error, info};
use tracing_subscriber;

const GUEST_CID: u32 = 3;
const GUEST_SERVICE_PORT: u32 = 1234;

#[derive(Error, Debug)]
enum AgentError {
    #[error("Vsock Bind/Accept Error: {0}")]
    VsockBind(std::io::Error),
    #[error("Vsock IO Error: {0}")]
    VsockIo(#[from] std::io::Error),
    #[error("Serialization Error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Command Execution Error: {0}")]
    CommandExec(String),
    #[error("Failed to capture stdio: {0}")]
    StdioCapture(String),
    #[error("Task Join Error: {0}")]
    JoinError(String),
}

fn map_join_error<T>(
    res: Result<T, tokio::task::JoinError>,
    task_name: &str,
) -> Result<T, AgentError> {
    res.map_err(|e| AgentError::JoinError(format!("{} task join error: {}", task_name, e)))
}

async fn handle_connection(mut stream: VsockStream) -> Result<(), AgentError> {
    info!("Accepted vsock connection");

    // 1. Read SandboxConfig
    let mut buffer = Vec::new();
    stream.read_to_end(&mut buffer).await?;
    let config: SandboxConfig = serde_json::from_slice(&buffer)?;
    info!(config=?config, "Received sandbox config");

    // 2. Execute command
    info!(command=?config.command, "Executing command...");
    let mut command = Command::new(&config.command[0]);
    command.args(&config.command[1..]);
    command.envs(
        config
            .env_vars
            .unwrap_or_default()
            .into_iter()
            .filter_map(|s| {
                s.split_once('=')
                    .map(|(k, v)| (k.to_string(), v.to_string()))
            }),
    );
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| AgentError::CommandExec(format!("Failed to spawn command: {}", e)))?;

    let stdin_opt = child.stdin.take();
    let stdout_opt = child.stdout.take();
    let stderr_opt = child.stderr.take();

    // Async IO Tasks
    let stdin_handle = tokio::spawn(async move {
        if let Some(mut stdin) = stdin_opt {
            let payload = config.payload; // Move payload
            match stdin.write_all(&payload).await {
                Ok(_) => stdin.shutdown().await,
                Err(e) => Err(e),
            }
        } else {
            Ok(()) // No stdin to write to
        }
    });

    let stdout_handle = tokio::spawn(async move {
        if let Some(mut stdout) = stdout_opt {
            let mut buf = Vec::new();
            stdout.read_to_end(&mut buf).await.map(|_| buf)
        } else {
            Ok(Vec::new())
        }
    });

    let stderr_handle = tokio::spawn(async move {
        if let Some(mut stderr) = stderr_opt {
            let mut buf = Vec::new();
            stderr.read_to_end(&mut buf).await.map(|_| buf)
        } else {
            Ok(Vec::new())
        }
    });

    // Wait for process completion and stdio tasks
    let (status_res, stdin_res, stdout_res, stderr_res) =
        tokio::join!(child.wait(), stdin_handle, stdout_handle, stderr_handle);

    let status =
        status_res.map_err(|e| AgentError::CommandExec(format!("Command wait failed: {}", e)))?;
    info!(exit_code=?status.code(), "Command finished");

    // Check task results
    if let Err(e) = map_join_error(stdin_res, "Stdin")? {
        error!(error = %e, "Error writing stdin or shutting down");
        // Non-fatal for now, process already finished
    }
    let stdout_data = map_join_error(stdout_res, "Stdout")??; // Inner ? handles IO error
    let stderr_data = map_join_error(stderr_res, "Stderr")??; // Inner ? handles IO error

    // Combine logs
    let mut combined_logs = Vec::new();
    combined_logs.extend_from_slice(b"STDOUT:\n");
    combined_logs.extend_from_slice(&stdout_data);
    combined_logs.extend_from_slice(b"\nSTDERR:\n");
    combined_logs.extend_from_slice(&stderr_data);
    let logs_string = String::from_utf8_lossy(&combined_logs).to_string();

    // 3. Construct InvocationResult
    let result = InvocationResult {
        request_id: config.function_id,
        response: Some(stdout_data),
        logs: Some(logs_string),
        error: if status.success() {
            None
        } else {
            // Include stderr in error message if process failed
            Some(format!(
                "Command failed with status: {}. Stderr: {}",
                status,
                String::from_utf8_lossy(&stderr_data)
            ))
        },
    };

    // 4. Send result back
    info!(result=?result, "Sending invocation result...");
    let result_json = serde_json::to_vec(&result)?;
    stream.write_all(&result_json);
    stream.shutdown(Shutdown::Both)?;

    info!("Finished handling connection.");
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting FaaS Guest Agent on Vsock...");

    let mut listener = match VsockListener::bind(GUEST_CID, GUEST_SERVICE_PORT) {
        Ok(l) => l,
        Err(e) => {
            error!(error=%e, cid=GUEST_CID, port=GUEST_SERVICE_PORT, "Failed to bind to vsock");
            return;
        }
    };
    info!(
        cid = GUEST_CID,
        port = GUEST_SERVICE_PORT,
        "Listening on vsock"
    );

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!(peer_addr=?addr, "Accepted vsock connection");
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream).await {
                        error!(error = %e, "Error handling connection");
                    }
                });
            }
            Err(e) => {
                error!(error = %e, "Failed to accept vsock connection");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
