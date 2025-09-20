
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::{sleep, timeout};

/// Custom error type that implements Send for use with tokio::spawn
#[derive(Debug, Clone)]
pub struct ReadinessError {
    pub message: String,
}

impl std::fmt::Display for ReadinessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ReadinessError {}

impl From<String> for ReadinessError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for ReadinessError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

/// Readiness check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessConfig {
    pub check_interval: Duration,
    pub initial_delay: Duration,
    pub timeout: Duration,
    pub success_threshold: u32,
    pub failure_threshold: u32,
    pub probes: Vec<ReadinessProbe>,
}

impl Default for ReadinessConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(1),
            initial_delay: Duration::from_secs(0),
            timeout: Duration::from_secs(60),
            success_threshold: 1,
            failure_threshold: 3,
            probes: vec![ReadinessProbe::default()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessProbe {
    pub probe_type: ProbeType,
    pub path: Option<String>,
    pub port: Option<u16>,
    pub command: Option<Vec<String>>,
    pub expected_status: Option<i32>,
    pub timeout: Duration,
}

impl Default for ReadinessProbe {
    fn default() -> Self {
        Self {
            probe_type: ProbeType::Tcp,
            path: None,
            port: Some(80),
            command: None,
            expected_status: Some(0),
            timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeType {
    Http,
    Tcp,
    Command,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessStatus {
    pub ready: bool,
    pub checks_performed: u32,
    pub consecutive_successes: u32,
    pub consecutive_failures: u32,
    pub last_check: Option<DateTime<Utc>>,
    pub message: String,
}

impl Default for ReadinessStatus {
    fn default() -> Self {
        Self {
            ready: false,
            checks_performed: 0,
            consecutive_successes: 0,
            consecutive_failures: 0,
            last_check: None,
            message: "Not checked yet".to_string(),
        }
    }
}

/// Readiness checker for instances
pub struct ReadinessChecker {
    config: ReadinessConfig,
    status: ReadinessStatus,
}

impl ReadinessChecker {
    pub fn new(config: ReadinessConfig) -> Self {
        Self {
            config,
            status: ReadinessStatus::default(),
        }
    }

    /// Wait for instance to be ready
    pub async fn wait_for_ready(
        &mut self,
        instance_id: &str,
    ) -> Result<ReadinessStatus, ReadinessError> {
        // Initial delay
        if self.config.initial_delay > Duration::ZERO {
            sleep(self.config.initial_delay).await;
        }

        let check_result = timeout(self.config.timeout, async {
            loop {
                let probe_result = self.perform_checks(instance_id).await;
                self.update_status(probe_result);

                if self.status.ready {
                    return Ok(self.status.clone());
                }

                if self.status.consecutive_failures >= self.config.failure_threshold {
                    return Err(format!(
                        "Readiness check failed after {} consecutive failures",
                        self.config.failure_threshold
                    )
                    .into());
                }

                sleep(self.config.check_interval).await;
            }
        })
        .await;

        match check_result {
            Ok(Ok(status)) => Ok(status),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                Err(format!("Readiness check timed out after {:?}", self.config.timeout).into())
            }
        }
    }

    /// Perform all configured probes
    async fn perform_checks(&self, instance_id: &str) -> bool {
        for probe in &self.config.probes {
            if !self.perform_single_probe(instance_id, probe).await {
                return false;
            }
        }
        true
    }

    /// Perform a single probe
    async fn perform_single_probe(&self, instance_id: &str, probe: &ReadinessProbe) -> bool {
        let probe_timeout = timeout(probe.timeout, async {
            match probe.probe_type {
                ProbeType::Http => self.check_http(instance_id, probe).await,
                ProbeType::Tcp => self.check_tcp(instance_id, probe).await,
                ProbeType::Command => self.check_command(instance_id, probe).await,
                ProbeType::File => self.check_file(instance_id, probe).await,
            }
        })
        .await;

        match probe_timeout {
            Ok(result) => result,
            Err(_) => {
                tracing::debug!("Probe timeout for instance {}", instance_id);
                false
            }
        }
    }

    /// HTTP readiness check
    async fn check_http(&self, instance_id: &str, probe: &ReadinessProbe) -> bool {
        let port = probe.port.unwrap_or(80);
        let path = probe.path.as_deref().unwrap_or("/health");
        let url = format!("http://{}:{}{}", instance_id, port, path);

        match reqwest::get(&url).await {
            Ok(response) => {
                let expected = probe.expected_status.unwrap_or(200) as u16;
                response.status().as_u16() == expected
            }
            Err(e) => {
                tracing::debug!("HTTP check failed for {}: {}", instance_id, e);
                false
            }
        }
    }

    /// TCP port readiness check
    async fn check_tcp(&self, instance_id: &str, probe: &ReadinessProbe) -> bool {
        use tokio::net::TcpStream;

        let port = probe.port.unwrap_or(80);
        let addr = format!("{}:{}", instance_id, port);

        match TcpStream::connect(&addr).await {
            Ok(_) => true,
            Err(e) => {
                tracing::debug!("TCP check failed for {}: {}", addr, e);
                false
            }
        }
    }

    /// Command execution readiness check
    async fn check_command(&self, instance_id: &str, probe: &ReadinessProbe) -> bool {
        use tokio::process::Command;

        let command = match &probe.command {
            Some(cmd) if !cmd.is_empty() => cmd,
            _ => return false,
        };

        let output = Command::new(&command[0])
            .args(&command[1..])
            .env("INSTANCE_ID", instance_id)
            .output()
            .await;

        match output {
            Ok(output) => {
                let expected = probe.expected_status.unwrap_or(0);
                output.status.code() == Some(expected)
            }
            Err(e) => {
                tracing::debug!("Command check failed for {}: {}", instance_id, e);
                false
            }
        }
    }

    /// File existence readiness check
    async fn check_file(&self, instance_id: &str, probe: &ReadinessProbe) -> bool {
        use tokio::fs;

        let path = match &probe.path {
            Some(p) => format!("/tmp/instances/{}/{}", instance_id, p),
            None => return false,
        };

        match fs::metadata(&path).await {
            Ok(_) => true,
            Err(e) => {
                tracing::debug!("File check failed for {}: {}", path, e);
                false
            }
        }
    }

    /// Update readiness status based on probe result
    fn update_status(&mut self, success: bool) {
        self.status.checks_performed += 1;
        self.status.last_check = Some(Utc::now());

        if success {
            self.status.consecutive_successes += 1;
            self.status.consecutive_failures = 0;

            if self.status.consecutive_successes >= self.config.success_threshold {
                self.status.ready = true;
                self.status.message = "Instance is ready".to_string();
            } else {
                self.status.message = format!(
                    "Waiting for {} more successful checks",
                    self.config.success_threshold - self.status.consecutive_successes
                );
            }
        } else {
            self.status.consecutive_successes = 0;
            self.status.consecutive_failures += 1;
            self.status.ready = false;
            self.status.message = format!(
                "Check failed ({}/{})",
                self.status.consecutive_failures, self.config.failure_threshold
            );
        }
    }
}

/// Long-running execution support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongRunningConfig {
    pub max_duration: Duration,
    pub heartbeat_interval: Duration,
    pub checkpoint_interval: Option<Duration>,
    pub auto_extend: bool,
    pub grace_period: Duration,
}

impl Default for LongRunningConfig {
    fn default() -> Self {
        Self {
            max_duration: Duration::from_secs(24 * 60 * 60), // 24 hours
            heartbeat_interval: Duration::from_secs(30),
            checkpoint_interval: Some(Duration::from_secs(300)), // 5 minutes
            auto_extend: false,
            grace_period: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSession {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub checkpoints: Vec<CheckpointInfo>,
    pub extended_count: u32,
    pub max_extensions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointInfo {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
}

/// Manager for long-running executions
pub struct LongRunningExecutionManager {
    config: LongRunningConfig,
    sessions:
        std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, ExecutionSession>>>,
}

impl LongRunningExecutionManager {
    pub fn new(config: LongRunningConfig) -> Self {
        Self {
            config,
            sessions: std::sync::Arc::new(tokio::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    /// Start a long-running execution session
    pub async fn start_session(
        &self,
        execution_id: String,
    ) -> Result<ExecutionSession, Box<dyn std::error::Error>> {
        let session = ExecutionSession {
            id: execution_id.clone(),
            started_at: Utc::now(),
            last_heartbeat: Utc::now(),
            checkpoints: vec![],
            extended_count: 0,
            max_extensions: 3,
        };

        self.sessions
            .write()
            .await
            .insert(execution_id.clone(), session.clone());

        // Start monitoring task
        self.spawn_monitor(execution_id);

        Ok(session)
    }

    /// Update heartbeat for session
    pub async fn heartbeat(&self, execution_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(execution_id) {
            session.last_heartbeat = Utc::now();
            Ok(())
        } else {
            Err(format!("Session {} not found", execution_id).into())
        }
    }

    /// Create checkpoint for session
    pub async fn checkpoint(
        &self,
        execution_id: &str,
        checkpoint_id: String,
        size_bytes: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(execution_id) {
            session.checkpoints.push(CheckpointInfo {
                id: checkpoint_id,
                created_at: Utc::now(),
                size_bytes,
            });
            Ok(())
        } else {
            Err(format!("Session {} not found", execution_id).into())
        }
    }

    /// Extend session duration
    pub async fn extend_session(
        &self,
        execution_id: &str,
    ) -> Result<Duration, Box<dyn std::error::Error>> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(execution_id) {
            if session.extended_count >= session.max_extensions {
                return Err("Maximum extensions reached".into());
            }

            session.extended_count += 1;
            Ok(self.config.max_duration)
        } else {
            Err(format!("Session {} not found", execution_id).into())
        }
    }

    /// Spawn monitoring task for session
    fn spawn_monitor(&self, execution_id: String) {
        let sessions = self.sessions.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            loop {
                sleep(config.heartbeat_interval).await;

                let should_terminate = {
                    let sessions = sessions.read().await;

                    if let Some(session) = sessions.get(&execution_id) {
                        let elapsed = Utc::now() - session.started_at;
                        let since_heartbeat = Utc::now() - session.last_heartbeat;

                        // Check if exceeded max duration
                        if elapsed > chrono::Duration::from_std(config.max_duration).unwrap() {
                            tracing::info!("Session {} exceeded max duration", execution_id);
                            true
                        }
                        // Check if heartbeat timeout
                        else if since_heartbeat
                            > chrono::Duration::from_std(config.heartbeat_interval * 3).unwrap()
                        {
                            tracing::info!("Session {} heartbeat timeout", execution_id);
                            true
                        } else {
                            false
                        }
                    } else {
                        true // Session removed
                    }
                };

                if should_terminate {
                    sessions.write().await.remove(&execution_id);
                    break;
                }

                // Automatic checkpointing
                if let Some(checkpoint_interval) = config.checkpoint_interval {
                    let should_checkpoint = {
                        let sessions = sessions.read().await;
                        if let Some(session) = sessions.get(&execution_id) {
                            if let Some(last_checkpoint) = session.checkpoints.last() {
                                let since_checkpoint = Utc::now() - last_checkpoint.created_at;
                                since_checkpoint
                                    > chrono::Duration::from_std(checkpoint_interval).unwrap()
                            } else {
                                true
                            }
                        } else {
                            false
                        }
                    };

                    if should_checkpoint {
                        tracing::debug!("Auto-checkpoint for session {}", execution_id);
                        // Trigger checkpoint (would call executor)
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_readiness_checker() {
        let config = ReadinessConfig {
            check_interval: Duration::from_millis(100),
            timeout: Duration::from_secs(1),
            success_threshold: 2,
            failure_threshold: 3,
            probes: vec![ReadinessProbe {
                probe_type: ProbeType::File,
                path: Some("ready.txt".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut checker = ReadinessChecker::new(config);

        // Create test instance directory
        let instance_dir = "/tmp/instances/test-ready";
        tokio::fs::create_dir_all(instance_dir).await.ok();

        // Start check in background
        let check_handle = tokio::spawn(async move { checker.wait_for_ready("test-ready").await });

        // Create ready file after delay
        tokio::time::sleep(Duration::from_millis(150)).await;
        tokio::fs::write(format!("{}/ready.txt", instance_dir), b"ready")
            .await
            .unwrap();

        let result = check_handle.await.unwrap();
        assert!(result.is_ok());

        let status = result.unwrap();
        assert!(status.ready);
        assert!(status.consecutive_successes >= 2);
    }

    #[tokio::test]
    async fn test_long_running_session() {
        let config = LongRunningConfig {
            max_duration: Duration::from_secs(60),
            heartbeat_interval: Duration::from_millis(100),
            ..Default::default()
        };

        let manager = LongRunningExecutionManager::new(config);

        let session = manager.start_session("exec-123".to_string()).await.unwrap();
        assert_eq!(session.id, "exec-123");

        // Send heartbeats
        for _ in 0..3 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            manager.heartbeat("exec-123").await.unwrap();
        }

        // Create checkpoint
        manager
            .checkpoint("exec-123", "ckpt-1".to_string(), 1024)
            .await
            .unwrap();

        let sessions = manager.sessions.read().await;
        let session = sessions.get("exec-123").unwrap();
        assert_eq!(session.checkpoints.len(), 1);
    }

    #[tokio::test]
    async fn test_session_extension() {
        let config = LongRunningConfig::default();
        let manager = LongRunningExecutionManager::new(config);

        manager.start_session("exec-456".to_string()).await.unwrap();

        // Extend session
        let new_duration = manager.extend_session("exec-456").await.unwrap();
        assert_eq!(new_duration, Duration::from_secs(24 * 60 * 60));

        let sessions = manager.sessions.read().await;
        let session = sessions.get("exec-456").unwrap();
        assert_eq!(session.extended_count, 1);
    }
}
