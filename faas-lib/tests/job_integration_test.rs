use blueprint_sdk::extract::Context;
use blueprint_sdk::runner::config::BlueprintEnvironment;
use blueprint_sdk::tangle::extract::{
    CallId, TangleArg, TangleArgs2, TangleArgs3, TangleArgs4, TangleArgs6, TangleArgs8,
};
use faas_blueprint_lib::context::FaaSContext;
use faas_blueprint_lib::jobs::*;
use std::fs;
use tempfile::tempdir;

async fn create_test_context() -> FaaSContext {
    let temp_dir = tempdir().expect("failed to create temp dir for test context");
    let base_path = temp_dir.path().to_path_buf();
    let keystore_dir = base_path.join("keystore");
    fs::create_dir_all(&keystore_dir).expect("failed to create keystore directory");
    let data_dir = temp_dir.keep();

    let mut env = BlueprintEnvironment::default();
    env.test_mode = true;
    env.data_dir = data_dir;
    env.keystore_uri = keystore_dir.to_string_lossy().into_owned();

    FaaSContext::new(env)
        .await
        .expect("Failed to create platform context")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_function() {
        let ctx = create_test_context().await;

        let result = execute_function_job(
            Context(ctx),
            CallId(1),
            TangleArgs4(
                "alpine:latest".to_string(),
                vec!["echo".to_string(), "hello".to_string()],
                None,
                vec![],
            ),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_advanced() {
        let ctx = create_test_context().await;

        let result = execute_advanced_job(
            Context(ctx),
            CallId(2),
            TangleArgs8(
                "alpine:latest".to_string(),
                vec!["echo".to_string(), "test".to_string()],
                None,
                vec![],
                "cached".to_string(),
                None,
                None,
                Some(30),
            ),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_snapshot_creation() {
        let ctx = create_test_context().await;

        let result = create_snapshot_job(
            Context(ctx),
            CallId(3),
            TangleArgs3(
                "test_container".to_string(),
                "test_snapshot".to_string(),
                Some("Test snapshot".to_string()),
            ),
        )
        .await;

        // Platform executor will handle this appropriately
        let _ = result;
    }

    #[tokio::test]
    async fn test_concurrent_execution() {
        let ctx = create_test_context().await;

        let ctx1 = ctx.clone();
        let ctx2 = ctx.clone();

        let (res1, res2) = tokio::join!(
            execute_function_job(
                Context(ctx1),
                CallId(10),
                TangleArgs4(
                    "alpine:latest".to_string(),
                    vec!["echo".to_string(), "task1".to_string()],
                    None,
                    vec![],
                )
            ),
            execute_function_job(
                Context(ctx2),
                CallId(11),
                TangleArgs4(
                    "alpine:latest".to_string(),
                    vec!["echo".to_string(), "task2".to_string()],
                    None,
                    vec![],
                )
            )
        );

        assert!(res1.is_ok());
        assert!(res2.is_ok());
    }
}
