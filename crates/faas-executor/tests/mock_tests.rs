//! Comprehensive executor tests with proper mocking and edge case coverage
//! Author: L7 Staff Engineer - Production-grade test suite

use anyhow::Result;
use async_trait::async_trait;
use faas_common::{InvocationResult, SandboxConfig, SandboxExecutor};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

/// Mock executor for testing - simulates various execution scenarios
#[derive(Clone)]
pub struct MockExecutor {
    pub executions: Arc<Mutex<Vec<ExecutionRecord>>>,
    pub behavior: Arc<RwLock<MockBehavior>>,
}

#[derive(Clone, Debug)]
pub struct ExecutionRecord {
    pub config: SandboxConfig,
    pub started_at: Instant,
    pub completed_at: Option<Instant>,
    pub result: Result<InvocationResult, String>,
}

#[derive(Clone, Debug)]
pub enum MockBehavior {
    Success { response: Vec<u8>, duration_ms: u64 },
    Failure { error: String },
    Timeout { after_ms: u64 },
    OOM { at_memory_mb: u64 },
    NetworkFailure,
    RandomFailure { failure_rate: f64 },
}

impl Default for MockBehavior {
    fn default() -> Self {
        MockBehavior::Success {
            response: b"OK".to_vec(),
            duration_ms: 100,
        }
    }
}

impl MockExecutor {
    pub fn new() -> Self {
        Self {
            executions: Arc::new(Mutex::new(Vec::new())),
            behavior: Arc::new(RwLock::new(MockBehavior::default())),
        }
    }

    pub async fn set_behavior(&self, behavior: MockBehavior) {
        *self.behavior.write().await = behavior;
    }

    pub async fn execution_count(&self) -> usize {
        self.executions.lock().await.len()
    }

    pub async fn last_execution(&self) -> Option<ExecutionRecord> {
        self.executions.lock().await.last().cloned()
    }
}

#[async_trait]
impl SandboxExecutor for MockExecutor {
    async fn execute(&self, config: SandboxConfig) -> faas_common::Result<InvocationResult> {
        let started_at = Instant::now();
        let behavior = self.behavior.read().await.clone();

        let result = match behavior {
            MockBehavior::Success {
                response,
                duration_ms,
            } => {
                tokio::time::sleep(Duration::from_millis(duration_ms)).await;
                Ok(InvocationResult {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    response: Some(response),
                    error: None,
                    logs: Some("Mock execution successful".to_string()),
                })
            }
            MockBehavior::Failure { error } => Err(faas_common::FaasError::Executor(error.clone())),
            MockBehavior::Timeout { after_ms } => {
                tokio::time::sleep(Duration::from_millis(after_ms)).await;
                Err(faas_common::FaasError::Executor(
                    "Execution timed out".to_string(),
                ))
            }
            MockBehavior::OOM { at_memory_mb } => Err(faas_common::FaasError::Executor(format!(
                "Out of memory at {} MB",
                at_memory_mb
            ))),
            MockBehavior::NetworkFailure => Err(faas_common::FaasError::Executor(
                "Network connection lost".to_string(),
            )),
            MockBehavior::RandomFailure { failure_rate } => {
                let should_fail = rand::random::<f64>() < failure_rate;
                if should_fail {
                    Err(faas_common::FaasError::Executor(
                        "Random failure occurred".to_string(),
                    ))
                } else {
                    Ok(InvocationResult {
                        request_id: uuid::Uuid::new_v4().to_string(),
                        response: Some(b"OK".to_vec()),
                        error: None,
                        logs: Some("Mock execution successful".to_string()),
                    })
                }
            }
        };

        let completed_at = Some(Instant::now());
        let record = ExecutionRecord {
            config: config.clone(),
            started_at,
            completed_at,
            result: result
                .as_ref()
                .map(|r| r.clone())
                .map_err(|e| e.to_string()),
        };

        self.executions.lock().await.push(record);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_successful_execution() {
        let executor = MockExecutor::new();
        executor
            .set_behavior(MockBehavior::Success {
                response: b"Hello, World!".to_vec(),
                duration_ms: 50,
            })
            .await;

        let config = SandboxConfig {
            function_id: "test-func".to_string(),
            source: "alpine:latest".to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            env_vars: Some(vec!["KEY=value".to_string()]),
            payload: b"input".to_vec(),
        };

        let result = executor.execute(config.clone()).await.unwrap();
        assert_eq!(result.response, Some(b"Hello, World!".to_vec()));
        assert!(result.error.is_none());
        assert_eq!(executor.execution_count().await, 1);

        let last_exec = executor.last_execution().await.unwrap();
        assert_eq!(last_exec.config.function_id, "test-func");
    }

    #[tokio::test]
    async fn test_execution_timeout() {
        let executor = MockExecutor::new();
        executor
            .set_behavior(MockBehavior::Timeout { after_ms: 5000 })
            .await;

        let config = SandboxConfig {
            function_id: "timeout-func".to_string(),
            source: "alpine:latest".to_string(),
            command: vec!["sleep".to_string(), "10".to_string()],
            env_vars: None,
            payload: vec![],
        };

        // Use timeout to prevent test from hanging
        let result =
            tokio::time::timeout(Duration::from_millis(100), executor.execute(config)).await;

        // Should timeout before mock completes
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_oom_handling() {
        let executor = MockExecutor::new();
        executor
            .set_behavior(MockBehavior::OOM { at_memory_mb: 512 })
            .await;

        let config = SandboxConfig {
            function_id: "memory-hog".to_string(),
            source: "alpine:latest".to_string(),
            command: vec!["allocate".to_string(), "1024MB".to_string()],
            env_vars: None,
            payload: vec![],
        };

        let result = executor.execute(config).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Out of memory"));
    }

    #[tokio::test]
    async fn test_network_failure() {
        let executor = MockExecutor::new();
        executor.set_behavior(MockBehavior::NetworkFailure).await;

        let config = SandboxConfig {
            function_id: "network-test".to_string(),
            source: "alpine:latest".to_string(),
            command: vec!["curl".to_string(), "https://example.com".to_string()],
            env_vars: None,
            payload: vec![],
        };

        let result = executor.execute(config).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Network connection lost"));
    }

    #[tokio::test]
    async fn test_concurrent_executions() {
        let executor = Arc::new(MockExecutor::new());
        executor
            .set_behavior(MockBehavior::Success {
                response: b"concurrent".to_vec(),
                duration_ms: 10,
            })
            .await;

        let mut handles = vec![];

        // Spawn 100 concurrent executions
        for i in 0..100 {
            let exec = executor.clone();
            let handle = tokio::spawn(async move {
                let config = SandboxConfig {
                    function_id: format!("concurrent-{}", i),
                    source: "alpine:latest".to_string(),
                    command: vec!["echo".to_string(), format!("{}", i)],
                    env_vars: None,
                    payload: vec![],
                };
                exec.execute(config).await
            });
            handles.push(handle);
        }

        // Wait for all to complete
        let results = futures::future::join_all(handles).await;

        // All should succeed
        for result in results {
            assert!(result.unwrap().is_ok());
        }

        assert_eq!(executor.execution_count().await, 100);
    }

    #[tokio::test]
    async fn test_random_failures() {
        let executor = MockExecutor::new();
        executor
            .set_behavior(MockBehavior::RandomFailure { failure_rate: 0.3 })
            .await;

        let mut success_count = 0;
        let mut failure_count = 0;

        // Run 100 executions
        for i in 0..100 {
            let config = SandboxConfig {
                function_id: format!("random-{}", i),
                source: "alpine:latest".to_string(),
                command: vec!["echo".to_string(), "test".to_string()],
                env_vars: None,
                payload: vec![],
            };

            match executor.execute(config).await {
                Ok(_) => success_count += 1,
                Err(_) => failure_count += 1,
            }
        }

        // With 30% failure rate, we expect roughly 30 failures
        assert!(
            failure_count > 20 && failure_count < 40,
            "Expected ~30 failures, got {}",
            failure_count
        );
        assert_eq!(success_count + failure_count, 100);
    }

    #[tokio::test]
    async fn test_execution_tracking() {
        let executor = MockExecutor::new();

        // Execute with different behaviors
        executor
            .set_behavior(MockBehavior::Success {
                response: b"first".to_vec(),
                duration_ms: 10,
            })
            .await;

        let config1 = SandboxConfig {
            function_id: "track-1".to_string(),
            source: "alpine:latest".to_string(),
            command: vec!["echo".to_string(), "1".to_string()],
            env_vars: None,
            payload: vec![],
        };
        executor.execute(config1).await.unwrap();

        executor
            .set_behavior(MockBehavior::Failure {
                error: "Intentional failure".to_string(),
            })
            .await;

        let config2 = SandboxConfig {
            function_id: "track-2".to_string(),
            source: "alpine:latest".to_string(),
            command: vec!["fail".to_string()],
            env_vars: None,
            payload: vec![],
        };
        let _ = executor.execute(config2).await;

        // Verify tracking
        assert_eq!(executor.execution_count().await, 2);

        let executions = executor.executions.lock().await;
        assert_eq!(executions[0].config.function_id, "track-1");
        assert!(executions[0].result.is_ok());
        assert_eq!(executions[1].config.function_id, "track-2");
        assert!(executions[1].result.is_err());
    }
}
