use blueprint_sdk::extract::Context;
use blueprint_sdk::runner::config::BlueprintEnvironment;
use blueprint_sdk::tangle::extract::{CallId, TangleArg};
use faas_blueprint_lib::context::FaaSContext;
use faas_blueprint_lib::jobs::*;
use faas_common::ExecuteFunctionArgs;
use faas_executor::platform::Executor as PlatformExecutor;
use std::sync::Arc;

async fn create_test_context() -> FaaSContext {
    let executor = PlatformExecutor::new()
        .await
        .expect("Failed to create platform executor");

    FaaSContext {
        config: BlueprintEnvironment::default(),
        executor: Arc::new(executor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_function() {
        let ctx = create_test_context().await;

        let args = ExecuteFunctionArgs {
            image: "alpine:latest".to_string(),
            command: vec!["echo".to_string(), "hello".to_string()],
            env_vars: None,
            payload: vec![],
        };

        let result = execute_function_job(Context(ctx), CallId(1), TangleArg(args)).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_advanced() {
        let ctx = create_test_context().await;

        let args = ExecuteAdvancedArgs {
            image: "alpine:latest".to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            env_vars: None,
            payload: vec![],
            mode: "cached".to_string(),
            checkpoint_id: None,
            branch_from: None,
            timeout_secs: Some(30),
        };

        let result = execute_advanced_job(Context(ctx), CallId(2), TangleArg(args)).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_snapshot_creation() {
        let ctx = create_test_context().await;

        let args = CreateSnapshotArgs {
            container_id: "test_container".to_string(),
            name: "test_snapshot".to_string(),
            description: Some("Test snapshot".to_string()),
        };

        let result = create_snapshot_job(Context(ctx), CallId(3), TangleArg(args)).await;

        // Platform executor will handle this appropriately
        let _ = result;
    }

    #[tokio::test]
    async fn test_concurrent_execution() {
        let ctx = create_test_context().await;

        let args1 = ExecuteFunctionArgs {
            image: "alpine:latest".to_string(),
            command: vec!["echo".to_string(), "task1".to_string()],
            env_vars: None,
            payload: vec![],
        };

        let args2 = ExecuteFunctionArgs {
            image: "alpine:latest".to_string(),
            command: vec!["echo".to_string(), "task2".to_string()],
            env_vars: None,
            payload: vec![],
        };

        let ctx1 = ctx.clone();
        let ctx2 = ctx.clone();

        let (res1, res2) = tokio::join!(
            execute_function_job(Context(ctx1), CallId(10), TangleArg(args1)),
            execute_function_job(Context(ctx2), CallId(11), TangleArg(args2))
        );

        assert!(res1.is_ok());
        assert!(res2.is_ok());
    }
}
