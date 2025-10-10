/// General-purpose WebSocket streaming system for real-time container communication
///
/// This module provides bidirectional streaming capabilities for ANY container:
/// - Stream stdout/stderr in real-time
/// - Send stdin commands via WebSocket
/// - Emit custom events (file changes, process events, etc.)
/// - Support multiple concurrent clients per container
///
/// Use cases:
/// - Vibecoding: AI agents streaming code changes
/// - CI/CD: Real-time build logs
/// - ML Training: Live metrics streaming
/// - Interactive shells: Terminal multiplexing
/// - Debug sessions: Live debugging output

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Maximum number of concurrent clients per container
const MAX_CLIENTS_PER_CONTAINER: usize = 100;

/// Broadcast channel buffer size
const BROADCAST_BUFFER_SIZE: usize = 1000;

/// Event types that can be streamed from containers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Standard output from container
    Stdout { data: String },

    /// Standard error from container
    Stderr { data: String },

    /// Container process exit
    Exit { code: i32 },

    /// File system event (created, modified, deleted)
    FileEvent {
        path: String,
        event: String, // "created", "modified", "deleted"
    },

    /// Process event (started, stopped)
    ProcessEvent {
        pid: u32,
        command: String,
        event: String, // "started", "stopped"
    },

    /// Custom application event
    Custom {
        name: String,
        data: serde_json::Value,
    },

    /// Heartbeat to keep connection alive
    Heartbeat,
}

/// Commands that can be sent TO containers via WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamCommand {
    /// Send stdin to container
    Stdin { data: String },

    /// Execute a command in the running container
    Exec { command: String },

    /// Request current container state
    GetState,

    /// Create a checkpoint/snapshot
    Checkpoint { name: Option<String> },

    /// Stop the container
    Stop,
}

/// Per-container streaming context
pub struct ContainerStream {
    /// Container ID
    pub container_id: String,

    /// Broadcast channel for events (one sender, many receivers)
    pub events_tx: broadcast::Sender<StreamEvent>,

    /// Number of active clients
    pub client_count: Arc<std::sync::atomic::AtomicUsize>,
}

/// Global streaming manager
pub struct StreamingManager {
    /// Active container streams (container_id -> ContainerStream)
    streams: Arc<DashMap<String, Arc<ContainerStream>>>,
}

impl StreamingManager {
    pub fn new() -> Self {
        Self {
            streams: Arc::new(DashMap::new()),
        }
    }

    /// Get or create a stream for a container
    pub fn get_or_create_stream(&self, container_id: String) -> Arc<ContainerStream> {
        self.streams
            .entry(container_id.clone())
            .or_insert_with(|| {
                let (events_tx, _) = broadcast::channel(BROADCAST_BUFFER_SIZE);
                Arc::new(ContainerStream {
                    container_id: container_id.clone(),
                    events_tx,
                    client_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                })
            })
            .clone()
    }

    /// Emit an event to all clients subscribed to a container
    pub fn emit_event(&self, container_id: &str, event: StreamEvent) {
        if let Some(stream) = self.streams.get(container_id) {
            // Non-blocking send - if no clients are listening, event is dropped
            let _ = stream.events_tx.send(event);
        }
    }

    /// Remove a container stream when the container is stopped
    pub fn remove_stream(&self, container_id: &str) {
        self.streams.remove(container_id);
    }

    /// Get current number of active streams
    pub fn active_streams_count(&self) -> usize {
        self.streams.len()
    }

    /// Get total number of connected clients across all streams
    pub fn total_clients(&self) -> usize {
        self.streams
            .iter()
            .map(|entry| entry.client_count.load(std::sync::atomic::Ordering::Relaxed))
            .sum()
    }
}

/// WebSocket upgrade handler
pub async fn ws_stream_handler(
    ws: WebSocketUpgrade,
    Path(container_id): Path<String>,
    State(manager): State<Arc<StreamingManager>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_websocket(socket, container_id, manager))
}

/// Handle individual WebSocket connection
async fn handle_websocket(
    socket: WebSocket,
    container_id: String,
    manager: Arc<StreamingManager>,
) {
    let stream = manager.get_or_create_stream(container_id.clone());

    // Increment client count
    let client_count = stream.client_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

    // Check max clients limit
    if client_count > MAX_CLIENTS_PER_CONTAINER {
        warn!("Max clients reached for container {}", container_id);
        stream.client_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        return;
    }

    info!("WebSocket connected to container {} (clients: {})", container_id, client_count);

    // Split socket into sender and receiver
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Subscribe to container events
    let mut events_rx = stream.events_tx.subscribe();

    // Spawn task to forward events to WebSocket
    let container_id_clone = container_id.clone();
    let forward_task = tokio::spawn(async move {
        while let Ok(event) = events_rx.recv().await {
            let json = match serde_json::to_string(&event) {
                Ok(j) => j,
                Err(e) => {
                    error!("Failed to serialize event: {}", e);
                    continue;
                }
            };

            if let Err(e) = ws_tx.send(Message::Text(json)).await {
                debug!("WebSocket send error for container {}: {}", container_id_clone, e);
                break;
            }
        }
    });

    // Handle incoming WebSocket messages (commands from client)
    let container_id_clone = container_id.clone();
    let manager_clone = manager.clone();
    let receive_task = tokio::spawn(async move {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<StreamCommand>(&text) {
                        Ok(cmd) => {
                            debug!("Received command for {}: {:?}", container_id_clone, cmd);
                            handle_command(&container_id_clone, cmd, &manager_clone).await;
                        }
                        Err(e) => {
                            warn!("Invalid command JSON: {}", e);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    debug!("WebSocket close for container {}", container_id_clone);
                    break;
                }
                Ok(Message::Ping(data)) => {
                    // Pongs are handled automatically by axum
                    debug!("Received ping for container {}", container_id_clone);
                }
                Err(e) => {
                    error!("WebSocket error for container {}: {}", container_id_clone, e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = forward_task => {},
        _ = receive_task => {},
    }

    // Decrement client count on disconnect
    stream.client_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    info!("WebSocket disconnected from container {}", container_id);
}

/// Handle commands sent from client to container
async fn handle_command(
    container_id: &str,
    command: StreamCommand,
    manager: &Arc<StreamingManager>,
) {
    match command {
        StreamCommand::Stdin { data } => {
            info!("Sending stdin to container {}: {:?}", container_id, data);
            // TODO: Implement actual stdin forwarding to Docker/Firecracker
            // This requires container executor API extension
        }

        StreamCommand::Exec { command } => {
            info!("Executing command in container {}: {}", container_id, command);
            // TODO: Execute command in running container and stream output
        }

        StreamCommand::GetState => {
            info!("Getting state for container {}", container_id);
            // TODO: Query container state and emit as event
            manager.emit_event(
                container_id,
                StreamEvent::Custom {
                    name: "state".to_string(),
                    data: serde_json::json!({
                        "status": "running",
                        "uptime": 12345,
                    }),
                },
            );
        }

        StreamCommand::Checkpoint { name } => {
            info!("Creating checkpoint for container {}: {:?}", container_id, name);
            // TODO: Trigger checkpoint/snapshot creation
        }

        StreamCommand::Stop => {
            info!("Stopping container {}", container_id);
            // TODO: Stop container gracefully
            manager.emit_event(container_id, StreamEvent::Exit { code: 0 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_manager_creation() {
        let manager = StreamingManager::new();
        assert_eq!(manager.active_streams_count(), 0);
    }

    #[test]
    fn test_get_or_create_stream() {
        let manager = StreamingManager::new();
        let stream1 = manager.get_or_create_stream("container-1".to_string());
        let stream2 = manager.get_or_create_stream("container-1".to_string());

        assert_eq!(stream1.container_id, stream2.container_id);
        assert_eq!(manager.active_streams_count(), 1);
    }

    #[test]
    fn test_emit_event() {
        let manager = StreamingManager::new();
        let stream = manager.get_or_create_stream("container-1".to_string());
        let mut rx = stream.events_tx.subscribe();

        manager.emit_event("container-1", StreamEvent::Stdout { data: "Hello".to_string() });

        let event = rx.try_recv().unwrap();
        match event {
            StreamEvent::Stdout { data } => assert_eq!(data, "Hello"),
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_remove_stream() {
        let manager = StreamingManager::new();
        manager.get_or_create_stream("container-1".to_string());
        assert_eq!(manager.active_streams_count(), 1);

        manager.remove_stream("container-1");
        assert_eq!(manager.active_streams_count(), 0);
    }
}
