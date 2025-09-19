//! Integration tests for all 12 Tangle jobs
//! Testing actual job execution flow with mocked executors

use async_trait::async_trait;
use blueprint_sdk::extract::Context;
use blueprint_sdk::tangle::extract::{CallId, TangleArg};
use faas_blueprint_lib::context::FaaSContext;
use faas_blueprint_lib::jobs::*;
use faas_common::{
    ExecuteFunctionArgs, InvocationResult, SandboxConfig, SandboxExecutor, FaasError, Result,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};

/// Test executor that records all executions for verification
struct TestExecutor {
    executions: Arc<Mutex<Vec<SandboxConfig>>>,
    responses: Arc<RwLock<HashMap<String, InvocationResult>>>,
}

impl TestExecutor {
    fn new() -> Self {
        Self {
            executions: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn set_response(&self, function_id: &str, result: InvocationResult) {
        self.responses.write().await.insert(function_id.to_string(), result);
    }

    async fn get_executions(&self) -> Vec<SandboxConfig> {
        self.executions.lock().await.clone()
    }
}

#[async_trait]
impl SandboxExecutor for TestExecutor {
    async fn execute(&self, config: SandboxConfig) -> Result<InvocationResult> {
        // Record the execution
        self.executions.lock().await.push(config.clone());
        
        // Return pre-configured response or default
        let responses = self.responses.read().await;
        if let Some(result) = responses.get(&config.function_id) {
            Ok(result.clone())
        } else {
            Ok(InvocationResult {
                request_id: uuid::Uuid::new_v4().to_string(),
                response: Some(b"test output".to_vec()),
                error: None,
                execution_time: Duration::from_millis(100),
                logs: format!("Executed {}", config.function_id),
            })
        }
    }
}

fn create_test_context() -> FaaSContext {
    let executor = Arc::new(TestExecutor::new());
    FaaSContext {
        sandbox_executor: executor,
        metrics: Default::default(),
        rate_limiter: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_function_job() {
        let ctx = create_test_context();
        let executor = ctx.sandbox_executor.clone();
        
        let args = ExecuteFunctionArgs {
            image: "alpine:latest".to_string(),
            command: vec!["echo".to_string(), "hello".to_string()],
            env_vars: Some(vec!["TEST=1".to_string()]),
            payload: b"input data".to_vec(),
        };

        let result = execute_function_job(
            Context(ctx),
            CallId(1),
            TangleArg(args.clone()),
        )
        .await;

        assert!(result.is_ok());
        
        // Verify execution was recorded
        let test_exec = executor.as_any()
            .downcast_ref::<TestExecutor>()
            .unwrap();
        let executions = test_exec.get_executions().await;
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].source, "alpine:latest");
        assert_eq!(executions[0].command, vec!["echo", "hello"]);
    }

    #[tokio::test]
    async fn test_execute_advanced_job_with_modes() {
        let ctx = create_test_context();
        
        // Test ephemeral mode
        let args = ExecuteAdvancedArgs {
            image: "rust:latest".to_string(),
            command: vec!["cargo".to_string(), "build".to_string()],
            env_vars: None,
            payload: vec![],
            mode: "ephemeral".to_string(),
            checkpoint_id: None,
            branch_from: None,
            timeout_secs: Some(30),
        };

        let result = execute_advanced_job(
            Context(ctx.clone()),
            CallId(2),
            TangleArg(args),
        )
        .await;
        assert!(result.is_ok());

        // Test cached mode
        let args = ExecuteAdvancedArgs {
            image: "node:20".to_string(),
            command: vec!["npm".to_string(), "test".to_string()],
            env_vars: None,
            payload: vec![],
            mode: "cached".to_string(),
            checkpoint_id: None,
            branch_from: None,
            timeout_secs: None,
        };

        let result = execute_advanced_job(
            Context(ctx.clone()),
            CallId(3),
            TangleArg(args),
        )
        .await;
        assert!(result.is_ok());

        // Test checkpointed mode with restore
        let args = ExecuteAdvancedArgs {
            image: "python:3.11".to_string(),
            command: vec!["python".to_string(), "train.py".to_string()],
            env_vars: None,
            payload: vec![],
            mode: "checkpointed".to_string(),
            checkpoint_id: Some("checkpoint_123".to_string()),
            branch_from: None,
            timeout_secs: Some(300),
        };

        let result = execute_advanced_job(
            Context(ctx),
            CallId(4),
            TangleArg(args),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_snapshot_job() {
        let ctx = create_test_context();
        
        let args = CreateSnapshotArgs {
            container_id: "container_abc123".to_string(),
            name: "dev-snapshot".to_string(),
            description: Some("Development environment snapshot".to_string()),
        };

        let result = create_snapshot_job(
            Context(ctx),
            CallId(5),
            TangleArg(args),
        )
        .await;

        assert!(result.is_ok());
        let snapshot_id = result.unwrap().0;
        assert!(snapshot_id.contains("snap_dev-snapshot"));
    }

    #[tokio::test]
    async fn test_restore_snapshot_job() {
        let ctx = create_test_context();
        let snapshot_id = "snap_dev-snapshot_123".to_string();

        let result = restore_snapshot_job(
            Context(ctx),
            CallId(6),
            TangleArg(snapshot_id.clone()),
        )
        .await;

        assert!(result.is_ok());
        let container_id = result.unwrap().0;
        assert!(container_id.contains("restored_"));
        assert!(container_id.contains(&snapshot_id));
    }

    #[tokio::test]
    async fn test_create_and_merge_branches() {
        let ctx = create_test_context();
        
        // Create branch
        let create_args = CreateBranchArgs {
            parent_snapshot_id: "snap_main_100".to_string(),
            branch_name: "feature-xyz".to_string(),
        };

        let create_result = create_branch_job(
            Context(ctx.clone()),
            CallId(7),
            TangleArg(create_args),
        )
        .await;
        
        assert!(create_result.is_ok());
        let branch_id = create_result.unwrap().0;
        assert!(branch_id.contains("branch_feature-xyz"));

        // Merge branches
        let merge_args = MergeBranchesArgs {
            source_branch_id: branch_id.clone(),
            target_branch_id: "branch_main".to_string(),
            strategy: "three-way".to_string(),
        };

        let merge_result = merge_branches_job(
            Context(ctx),
            CallId(8),
            TangleArg(merge_args),
        )
        .await;

        assert!(merge_result.is_ok());
        let merged_id = merge_result.unwrap().0;
        assert!(merged_id.contains("merged_"));
    }

    #[tokio::test]
    async fn test_instance_lifecycle() {
        let ctx = create_test_context();
        
        // Start instance
        let start_args = StartInstanceArgs {
            snapshot_id: None,
            image: Some("ubuntu:22.04".to_string()),
            cpu_cores: 2,
            memory_mb: 2048,
            disk_gb: 20,
            enable_ssh: true,
        };

        let start_result = start_instance_job(
            Context(ctx.clone()),
            CallId(9),
            TangleArg(start_args),
        )
        .await;

        assert!(start_result.is_ok());
        let instance_id = start_result.unwrap().0;
        assert!(instance_id.contains("instance_"));

        // Pause instance
        let pause_result = pause_instance_job(
            Context(ctx.clone()),
            CallId(10),
            TangleArg(instance_id.clone()),
        )
        .await;

        assert!(pause_result.is_ok());
        let checkpoint_id = pause_result.unwrap().0;
        assert!(checkpoint_id.contains("pause_"));

        // Resume instance
        let resume_result = resume_instance_job(
            Context(ctx.clone()),
            CallId(11),
            TangleArg(checkpoint_id),
        )
        .await;

        assert!(resume_result.is_ok());
        let resumed_id = resume_result.unwrap().0;
        assert!(resumed_id.contains("resumed_"));

        // Stop instance
        let stop_result = stop_instance_job(
            Context(ctx),
            CallId(12),
            TangleArg(instance_id),
        )
        .await;

        assert!(stop_result.is_ok());
        assert_eq!(stop_result.unwrap().0, true);
    }

    #[tokio::test]
    async fn test_expose_port_job() {
        let ctx = create_test_context();
        
        let args = ExposePortArgs {
            instance_id: "instance_abc".to_string(),
            internal_port: 8080,
            protocol: "https".to_string(),
            subdomain: Some("my-app".to_string()),
        };

        let result = expose_port_job(
            Context(ctx),
            CallId(13),
            TangleArg(args),
        )
        .await;

        assert!(result.is_ok());
        let public_url = result.unwrap().0;
        assert!(public_url.contains("https://my-app.faas.local:8080"));
    }

    #[tokio::test]
    async fn test_upload_files_job() {
        let ctx = create_test_context();
        
        let args = UploadFilesArgs {
            instance_id: "instance_xyz".to_string(),
            files: vec![
                ("src/main.rs".to_string(), b"fn main() {}".to_vec()),
                ("Cargo.toml".to_string(), b"[package]\nname = \"test\"".to_vec()),
            ],
            target_path: "/workspace".to_string(),
        };

        let result = upload_files_job(
            Context(ctx),
            CallId(14),
            TangleArg(args.clone()),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, 2); // Number of files uploaded
    }

    #[tokio::test]
    async fn test_job_error_handling() {
        let ctx = create_test_context();
        
        // Test with invalid image
        let args = ExecuteFunctionArgs {
            image: "".to_string(), // Invalid empty image
            command: vec![],
            env_vars: None,
            payload: vec![],
        };

        let result = execute_function_job(
            Context(ctx.clone()),
            CallId(15),
            TangleArg(args),
        )
        .await;

        // Should handle gracefully
        assert!(result.is_err() || result.unwrap().0.is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_job_execution() {
        let ctx = create_test_context();
        let mut handles = vec![];
        
        // Launch multiple jobs concurrently
        for i in 0..10 {
            let ctx_clone = ctx.clone();
            let handle = tokio::spawn(async move {
                let args = ExecuteFunctionArgs {
                    image: "alpine:latest".to_string(),
                    command: vec!["echo".to_string(), format!("{}", i)],
                    env_vars: None,
                    payload: vec![],
                };

                execute_function_job(
                    Context(ctx_clone),
                    CallId(100 + i),
                    TangleArg(args),
                )
                .await
            });
            handles.push(handle);
        }

        // All should complete successfully
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }
    }
}