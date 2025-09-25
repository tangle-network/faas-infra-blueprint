//! High-level SDK for building services on FaaS platform
//!
//! This SDK provides abstractions that make it easy to build
//! sophisticated services without touching the core platform.

use faas_executor::DockerExecutor;
use faas_common::{SandboxConfig, SandboxExecutor};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// High-level client for FaaS platform
pub struct FaaSClient {
    executor: DockerExecutor,
    snapshots: Arc<RwLock<HashMap<String, Snapshot>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    pub base_image: String,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    pub timeout: Duration,
    pub memory_mb: u32,
    pub cpu_cores: f32,
    pub gpu_enabled: bool,
    pub network_enabled: bool,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            memory_mb: 512,
            cpu_cores: 1.0,
            gpu_enabled: false,
            network_enabled: false,
        }
    }
}

impl FaaSClient {
    /// Create a new FaaS client
    pub fn new(executor: DockerExecutor) -> Self {
        Self {
            executor,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Execute a function with automatic snapshotting
    pub async fn execute_with_cache<F>(
        &self,
        cache_key: &str,
        setup: F,
        command: Vec<String>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnOnce() -> String,
    {
        // Check if we have a snapshot for this cache key
        let snapshots = self.snapshots.read().await;

        if let Some(snapshot) = snapshots.get(cache_key) {
            println!("⚡ Using cached snapshot: {}", snapshot.id);
            // In production: Restore from snapshot
        } else {
            drop(snapshots); // Release read lock

            println!("📸 Creating new snapshot for: {}", cache_key);
            let setup_script = setup();

            // Create snapshot
            let snapshot = Snapshot {
                id: format!("snap-{}-{}", cache_key, uuid::Uuid::new_v4()),
                created_at: Instant::now(),
                base_image: "ubuntu:latest".to_string(),
                metadata: HashMap::new(),
            };

            self.snapshots.write().await.insert(cache_key.to_string(), snapshot);
        }

        // Execute command
        let result = self.executor.execute(SandboxConfig {
            function_id: cache_key.to_string(),
            source: "ubuntu:latest".to_string(),
            command,
            env_vars: None,
            payload: vec![],
        }).await?;

        Ok(result.response.unwrap_or_default())
    }

    /// Branch execution for parallel exploration
    pub async fn branch<F>(
        &self,
        base_snapshot: &str,
        branches: Vec<(&str, F)>,
    ) -> Vec<Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>>
    where
        F: Fn() -> Vec<String> + Send + Sync + 'static,
    {
        let mut handles = vec![];

        for (branch_name, get_command) in branches {
            let executor = self.executor.clone();
            let name = branch_name.to_string();
            let command = get_command();

            handles.push(tokio::spawn(async move {
                let result = executor.execute(SandboxConfig {
                    function_id: name,
                    source: "ubuntu:latest".to_string(),
                    command,
                    env_vars: None,
                    payload: vec![],
                }).await;

                result.map(|r| r.response.unwrap_or_default())
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            }));
        }

        let results = futures::future::join_all(handles).await;
        results.into_iter()
            .map(|r| r.unwrap_or_else(|e| Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)))
            .collect()
    }
}

/// Builder pattern for complex workflows
pub struct WorkflowBuilder {
    steps: Vec<WorkflowStep>,
}

#[derive(Clone)]
struct WorkflowStep {
    name: String,
    image: String,
    command: Vec<String>,
    depends_on: Vec<String>,
}

impl WorkflowBuilder {
    pub fn new() -> Self {
        Self { steps: vec![] }
    }

    pub fn add_step(mut self, name: &str, image: &str, command: Vec<String>) -> Self {
        self.steps.push(WorkflowStep {
            name: name.to_string(),
            image: image.to_string(),
            command,
            depends_on: vec![],
        });
        self
    }

    pub fn with_dependency(mut self, step: &str, depends_on: &str) -> Self {
        if let Some(s) = self.steps.iter_mut().find(|s| s.name == step) {
            s.depends_on.push(depends_on.to_string());
        }
        self
    }

    pub async fn execute(self, client: &FaaSClient) -> Result<HashMap<String, Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
        let mut results = HashMap::new();

        // Simple sequential execution (in production: use DAG scheduler)
        for step in self.steps {
            println!("🔄 Executing step: {}", step.name);

            let result = client.executor.execute(SandboxConfig {
                function_id: step.name.clone(),
                source: step.image,
                command: step.command,
                env_vars: None,
                payload: vec![],
            }).await?;

            results.insert(step.name, result.response.unwrap_or_default());
        }

        Ok(results)
    }
}

/// Specialized GPU service builder
pub struct GpuServiceBuilder {
    model_name: String,
    framework: String,
    memory_gb: f32,
}

impl GpuServiceBuilder {
    pub fn new(model_name: &str) -> Self {
        Self {
            model_name: model_name.to_string(),
            framework: "pytorch".to_string(),
            memory_gb: 4.0,
        }
    }

    pub fn with_framework(mut self, framework: &str) -> Self {
        self.framework = framework.to_string();
        self
    }

    pub fn with_memory(mut self, gb: f32) -> Self {
        self.memory_gb = gb;
        self
    }

    pub fn build(self) -> String {
        format!(
            "GPU Service: {} ({}) with {}GB memory",
            self.model_name, self.framework, self.memory_gb
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_builder() {
        let workflow = WorkflowBuilder::new()
            .add_step("fetch", "alpine", vec!["wget".to_string(), "data.csv".to_string()])
            .add_step("process", "python:3.11", vec!["python".to_string(), "process.py".to_string()])
            .add_step("analyze", "r-base", vec!["Rscript".to_string(), "analyze.R".to_string()])
            .with_dependency("process", "fetch")
            .with_dependency("analyze", "process");

        assert_eq!(workflow.steps.len(), 3);
    }

    #[test]
    fn test_gpu_service_builder() {
        let service = GpuServiceBuilder::new("llama-7b")
            .with_framework("transformers")
            .with_memory(16.0)
            .build();

        assert!(service.contains("llama-7b"));
        assert!(service.contains("16"));
    }
}