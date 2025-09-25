use crate::executor_wrapper::{ExecutionConfig, ExecutionResult};
use crate::types::{Instance, Snapshot};
use crate::{create_app, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bollard::Docker;
use dashmap::DashMap;
use faas_executor::DockerExecutor;
use faas_gateway::{ExecuteRequest, InvokeResponse};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

#[cfg(test)]
mod gateway_tests {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "healthy");
        assert!(json["timestamp"].is_string());
    }

    #[tokio::test]
    async fn test_execute_endpoint() {
        let app = create_test_app().await;

        let request_body = json!({
            "command": "echo 'test'",
            "image": "alpine:latest"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/execute")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let result: InvokeResponse = serde_json::from_slice(&body).unwrap();

        assert!(result.request_id.len() > 0);
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_execute_advanced_modes() {
        let app = create_test_app().await;

        let modes = vec!["ephemeral", "cached", "persistent"];

        for mode in modes {
            let request_body = json!({
                "command": "date",
                "image": "alpine:latest",
                "mode": mode
            });

            let response = app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/execute/advanced")
                        .header("content-type", "application/json")
                        .body(Body::from(request_body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);

            let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let result: InvokeResponse = serde_json::from_slice(&body).unwrap();

            assert!(result.request_id.len() > 0);
        }
    }

    #[tokio::test]
    async fn test_snapshot_lifecycle() {
        let app = create_test_app().await;

        // Create snapshot
        let create_body = json!({
            "name": "test-snapshot",
            "container_id": "test-container"
        });

        let create_response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/snapshots")
                    .header("content-type", "application/json")
                    .body(Body::from(create_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(create_response.into_body()).await.unwrap();
        let snapshot: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let snapshot_id = snapshot["id"].as_str().unwrap();

        // List snapshots
        let list_response = app.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/snapshots")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(list_response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(list_response.into_body()).await.unwrap();
        let snapshots: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(snapshots.len() > 0);

        // Restore snapshot
        let restore_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/snapshots/{}/restore", snapshot_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(restore_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_instance_lifecycle() {
        let app = create_test_app().await;

        // Create instance
        let create_body = json!({
            "image": "alpine:latest",
            "cpu_cores": 1,
            "memory_mb": 512
        });

        let create_response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/instances")
                    .header("content-type", "application/json")
                    .body(Body::from(create_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(create_response.into_body()).await.unwrap();
        let instance: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let instance_id = instance["id"].as_str().unwrap();

        // Get instance
        let get_response = app.clone()
            .oneshot(
                Request::builder()
                    .uri(&format!("/api/v1/instances/{}", instance_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(get_response.status(), StatusCode::OK);

        // Execute in instance
        let exec_body = json!({
            "command": "uname -a"
        });

        let exec_response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/instances/{}/exec", instance_id))
                    .header("content-type", "application/json")
                    .body(Body::from(exec_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(exec_response.status(), StatusCode::OK);

        // Stop instance
        let stop_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/instances/{}/stop", instance_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(stop_response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let app = create_test_app().await;

        // Test invalid JSON
        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/execute")
                    .header("content-type", "application/json")
                    .body(Body::from("invalid json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Test missing required fields
        let incomplete_body = json!({
            "image": "alpine:latest"
            // missing command
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/execute")
                    .header("content-type", "application/json")
                    .body(Body::from(incomplete_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_platform_specific_modes() {
        let app = create_test_app().await;

        // Test checkpointed mode (should fallback on non-Linux)
        let request_body = json!({
            "command": "echo 'checkpoint test'",
            "image": "alpine:latest",
            "mode": "checkpointed"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/execute/advanced")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let result: InvokeResponse = serde_json::from_slice(&body).unwrap();

        // Should succeed even on non-Linux (with fallback)
        assert_eq!(result.exit_code, 0);
    }

    // Helper function to create test app
    async fn create_test_app() -> Router {
        use crate::executor_wrapper::ExecutorWrapper;

        let docker = Docker::connect_with_local_defaults().unwrap();
        let executor = Arc::new(DockerExecutor::new(Arc::new(docker)));
        let executor_wrapper = Arc::new(ExecutorWrapper::new(executor.clone()));

        let state = AppState {
            executor_wrapper,
            executor,
            instances: Arc::new(DashMap::new()),
            snapshots: Arc::new(DashMap::new()),
        };

        create_app(state)
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_execution_config_creation() {
        let config = ExecutionConfig {
            image: "alpine:latest".to_string(),
            command: "echo test".to_string(),
            env_vars: vec![("KEY".to_string(), "value".to_string())],
            working_dir: Some("/app".to_string()),
            timeout: std::time::Duration::from_secs(30),
            memory_limit: Some(1024),
            cpu_limit: Some(1.5),
        };

        assert_eq!(config.image, "alpine:latest");
        assert_eq!(config.command, "echo test");
        assert_eq!(config.env_vars.len(), 1);
        assert_eq!(config.timeout.as_secs(), 30);
    }

    #[test]
    fn test_execution_result_creation() {
        let result = ExecutionResult {
            exit_code: 0,
            stdout: "output".to_string(),
            stderr: String::new(),
            duration: std::time::Duration::from_millis(100),
        };

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "output");
        assert!(result.stderr.is_empty());
        assert_eq!(result.duration.as_millis(), 100);
    }
}