/// WebSocket Streaming Demo
///
/// This example demonstrates the general-purpose WebSocket streaming API.
/// It shows how ANY container can stream bidirectional data in real-time.
///
/// Use cases demonstrated:
/// - Real-time stdout/stderr streaming
/// - Interactive command execution
/// - Custom event streaming
/// - Live container state monitoring

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEvent {
    Stdout { data: String },
    Stderr { data: String },
    Exit { code: i32 },
    FileEvent { path: String, event: String },
    ProcessEvent { pid: u32, command: String, event: String },
    Custom { name: String, data: serde_json::Value },
    Heartbeat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamCommand {
    Stdin { data: String },
    Exec { command: String },
    GetState,
    Checkpoint { name: Option<String> },
    Stop,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("🚀 WebSocket Streaming Demo");
    info!("==========================");

    // Step 1: Start a persistent container via HTTP API
    let client = reqwest::Client::new();
    let container_id = create_persistent_container(&client).await?;
    info!("✅ Created persistent container: {}", container_id);

    // Step 2: Connect to WebSocket stream
    let ws_url = format!("ws://localhost:8080/api/v1/containers/{}/stream", container_id);
    info!("🔌 Connecting to WebSocket: {}", ws_url);

    let (ws_stream, _) = connect_async(&ws_url).await?;
    info!("✅ WebSocket connected");

    let (mut write, mut read) = ws_stream.split();

    // Step 3: Spawn task to read events from container
    let read_task = tokio::spawn(async move {
        info!("📡 Listening for container events...");
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<StreamEvent>(&text) {
                        Ok(event) => {
                            match event {
                                StreamEvent::Stdout { data } => {
                                    info!("📤 STDOUT: {}", data);
                                }
                                StreamEvent::Stderr { data } => {
                                    error!("📤 STDERR: {}", data);
                                }
                                StreamEvent::Exit { code } => {
                                    info!("🛑 Container exited with code: {}", code);
                                    break;
                                }
                                StreamEvent::FileEvent { path, event } => {
                                    info!("📁 File event: {} - {}", path, event);
                                }
                                StreamEvent::ProcessEvent { pid, command, event } => {
                                    info!("⚙️  Process event: {} ({}) - {}", command, pid, event);
                                }
                                StreamEvent::Custom { name, data } => {
                                    info!("🔔 Custom event: {} - {:?}", name, data);
                                }
                                StreamEvent::Heartbeat => {
                                    // Silent heartbeat
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse event: {}", e);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("🔌 WebSocket closed");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Step 4: Send commands to container
    info!("");
    info!("📨 Sending commands to container...");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Execute a command
    info!("  → Execute: echo 'Hello from FaaS!'");
    let cmd = StreamCommand::Exec {
        command: "echo 'Hello from FaaS!'".to_string(),
    };
    write.send(Message::Text(serde_json::to_string(&cmd)?)).await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Send stdin
    info!("  → Stdin: Interactive input");
    let stdin_cmd = StreamCommand::Stdin {
        data: "Interactive input test\n".to_string(),
    };
    write.send(Message::Text(serde_json::to_string(&stdin_cmd)?)).await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Get container state
    info!("  → Get State");
    let state_cmd = StreamCommand::GetState;
    write.send(Message::Text(serde_json::to_string(&state_cmd)?)).await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Create a checkpoint
    info!("  → Create Checkpoint: 'demo-checkpoint'");
    let checkpoint_cmd = StreamCommand::Checkpoint {
        name: Some("demo-checkpoint".to_string()),
    };
    write.send(Message::Text(serde_json::to_string(&checkpoint_cmd)?)).await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Stop container
    info!("  → Stop Container");
    let stop_cmd = StreamCommand::Stop;
    write.send(Message::Text(serde_json::to_string(&stop_cmd)?)).await?;

    // Wait for read task to complete
    read_task.await?;

    info!("");
    info!("✅ Demo completed successfully!");
    info!("");
    info!("Key Features Demonstrated:");
    info!("  • Bidirectional WebSocket communication");
    info!("  • Real-time event streaming");
    info!("  • Interactive command execution");
    info!("  • Container lifecycle management");
    info!("  • Checkpointing via WebSocket");

    Ok(())
}

/// Create a persistent container via HTTP API
async fn create_persistent_container(client: &reqwest::Client) -> Result<String> {
    let response = client
        .post("http://localhost:8080/api/v1/execute")
        .json(&serde_json::json!({
            "command": "sleep 30",
            "image": "alpine:latest",
            "mode": "persistent",
            "timeout_ms": 60000,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to create container: {}", response.status());
    }

    let body: serde_json::Value = response.json().await?;
    let container_id = body["request_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No request_id in response"))?
        .to_string();

    Ok(container_id)
}
