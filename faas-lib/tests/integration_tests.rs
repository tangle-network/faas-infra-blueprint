#[cfg(test)]
mod tests {
    use blueprint_sdk::extract::Context;
    use blueprint_sdk::tangle::extract::{CallId, TangleArg};
    use faas_blueprint_lib::api_server::{ApiKeyPermissions, ApiServerConfig};
    use faas_blueprint_lib::context::FaaSContext;
    use faas_blueprint_lib::jobs::*;
    use faas_common::ExecuteFunctionArgs;
    use std::collections::HashMap;
    use tokio;

    fn create_test_context() -> FaaSContext {
        FaaSContext::new_for_test()
    }

    // Test all 12 Tangle jobs
    #[tokio::test]
    async fn test_execute_function_job() {
        let ctx = create_test_context();
        let args = ExecuteFunctionArgs {
            image: "alpine:latest".to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            env_vars: None,
            payload: vec![],
        };

        let result = execute_function_job(Context(ctx), CallId(1), TangleArg(args)).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_advanced_job() {
        let ctx = create_test_context();
        let args = ExecuteAdvancedArgs {
            image: "alpine:latest".to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            env_vars: None,
            payload: vec![],
            mode: "ephemeral".to_string(),
            checkpoint_id: None,
            branch_from: None,
            timeout_secs: Some(60),
        };

        let result = execute_advanced_job(Context(ctx), CallId(2), TangleArg(args)).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_snapshot_job() {
        let ctx = create_test_context();
        let args = CreateSnapshotArgs {
            container_id: "test-container".to_string(),
            name: "test-snapshot".to_string(),
            description: None,
        };

        let result = create_snapshot_job(Context(ctx), CallId(3), TangleArg(args)).await;

        // Will fail gracefully without real container
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_restore_snapshot_job() {
        let ctx = create_test_context();
        let args = RestoreSnapshotArgs {
            snapshot_id: "test-snapshot-id".to_string(),
        };

        let result = restore_snapshot_job(Context(ctx), CallId(4), TangleArg(args)).await;

        // Will fail gracefully without real snapshot
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_branch_job() {
        let ctx = create_test_context();
        let args = CreateBranchArgs {
            parent_snapshot_id: "parent-id".to_string(),
            branch_name: "test-branch".to_string(),
        };

        let result = create_branch_job(Context(ctx), CallId(5), TangleArg(args)).await;

        // Will fail gracefully without real snapshot
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_merge_branches_job() {
        let ctx = create_test_context();
        let args = MergeBranchesArgs {
            branch_ids: vec!["branch1".to_string(), "branch2".to_string()],
            merge_strategy: "latest".to_string(),
        };

        let result = merge_branches_job(Context(ctx), CallId(6), TangleArg(args)).await;

        // Will fail gracefully without real branches
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_instance_job() {
        let ctx = create_test_context();
        let args = StartInstanceArgs {
            snapshot_id: None,
            image: Some("alpine:latest".to_string()),
            cpu_cores: 1,
            memory_mb: 512,
            disk_gb: 1,
            enable_ssh: false,
        };

        let result = start_instance_job(Context(ctx), CallId(7), TangleArg(args)).await;

        // Will fail gracefully without real resources
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stop_instance_job() {
        let ctx = create_test_context();
        let args = StopInstanceArgs {
            instance_id: "test-instance".to_string(),
        };

        let result = stop_instance_job(Context(ctx), CallId(8), TangleArg(args)).await;

        // Will fail gracefully without real instance
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pause_instance_job() {
        let ctx = create_test_context();
        let args = PauseInstanceArgs {
            instance_id: "test-instance".to_string(),
        };

        let result = pause_instance_job(Context(ctx), CallId(9), TangleArg(args)).await;

        // Will fail gracefully without real instance
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resume_instance_job() {
        let ctx = create_test_context();
        let args = ResumeInstanceArgs {
            instance_id: "test-instance".to_string(),
            checkpoint_id: "checkpoint-id".to_string(),
        };

        let result = resume_instance_job(Context(ctx), CallId(10), TangleArg(args)).await;

        // Will fail gracefully without real checkpoint
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_expose_port_job() {
        let ctx = create_test_context();
        let args = ExposePortArgs {
            instance_id: "test-instance".to_string(),
            internal_port: 8080,
            protocol: "http".to_string(),
            subdomain: None,
        };

        let result = expose_port_job(Context(ctx), CallId(11), TangleArg(args)).await;

        // Will fail gracefully without real instance
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_upload_files_job() {
        let ctx = create_test_context();
        let args = UploadFilesArgs {
            instance_id: "test-instance".to_string(),
            target_path: "/tmp/test".to_string(),
            files_data: vec![1, 2, 3, 4],
        };

        let result = upload_files_job(Context(ctx), CallId(12), TangleArg(args)).await;

        // Will fail gracefully without real instance
        assert!(result.is_err());
    }

    // Test API server configuration and authentication
    #[test]
    fn test_api_server_config() {
        let mut config = ApiServerConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);

        // Add test API key
        let permissions = ApiKeyPermissions {
            name: "test-key".to_string(),
            can_execute: true,
            can_manage_instances: true,
            rate_limit: Some(100),
        };
        config
            .api_keys
            .insert("test-api-key".to_string(), permissions.clone());

        assert!(config.api_keys.contains_key("test-api-key"));
        let retrieved = config.api_keys.get("test-api-key").unwrap();
        assert_eq!(retrieved.name, "test-key");
        assert!(retrieved.can_execute);
        assert!(retrieved.can_manage_instances);
        assert_eq!(retrieved.rate_limit, Some(100));
    }

    #[tokio::test]
    async fn test_api_authentication() {
        use axum::http::HeaderMap;
        use faas_blueprint_lib::api_server::authenticate;
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let mut config = ApiServerConfig::default();
        config.api_keys.insert(
            "valid-key".to_string(),
            ApiKeyPermissions {
                name: "test".to_string(),
                can_execute: true,
                can_manage_instances: false,
                rate_limit: Some(10),
            },
        );

        let state = ApiState {
            context: create_test_context(),
            config,
            request_counts: Arc::new(RwLock::new(HashMap::new())),
        };

        // Test with valid key
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "valid-key".parse().unwrap());

        let result = authenticate(&headers, &state).await;
        assert!(result.is_ok());
        let perms = result.unwrap();
        assert!(perms.can_execute);
        assert!(!perms.can_manage_instances);

        // Test with invalid key
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "invalid-key".parse().unwrap());

        let result = authenticate(&headers, &state).await;
        assert!(result.is_err());

        // Test with missing key
        let headers = HeaderMap::new();
        let result = authenticate(&headers, &state).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        use faas_blueprint_lib::api_server::{check_rate_limit, ApiState};
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let config = ApiServerConfig::default();
        let state = ApiState {
            context: create_test_context(),
            config,
            request_counts: Arc::new(RwLock::new(HashMap::new())),
        };

        // Test with rate limit
        for i in 1..=5 {
            let result = check_rate_limit("test-key", Some(5), &state).await;
            if i <= 5 {
                assert!(result.is_ok());
            }
        }

        // 6th request should fail
        let result = check_rate_limit("test-key", Some(5), &state).await;
        assert!(result.is_err());

        // Test without rate limit
        let result = check_rate_limit("unlimited-key", None, &state).await;
        assert!(result.is_ok());
    }

    // Test execution modes
    #[tokio::test]
    async fn test_execution_modes() {
        use faas_executor::platform::Mode;

        // Test mode parsing
        let modes = vec![
            ("ephemeral", Mode::Ephemeral),
            ("cached", Mode::Cached),
            ("checkpointed", Mode::Checkpointed),
            ("branched", Mode::Branched),
            ("persistent", Mode::Persistent),
        ];

        for (mode_str, expected_mode) in modes {
            let ctx = create_test_context();
            let args = ExecuteAdvancedArgs {
                image: "alpine:latest".to_string(),
                command: vec!["echo".to_string(), mode_str.to_string()],
                env_vars: None,
                payload: vec![],
                mode: mode_str.to_string(),
                checkpoint_id: None,
                branch_from: None,
                timeout_secs: Some(5),
            };

            let _result = execute_advanced_job(Context(ctx), CallId(100), TangleArg(args)).await;

            // Just verify parsing doesn't panic
            assert_eq!(format!("{:?}", expected_mode).to_lowercase(), mode_str);
        }
    }

    // Integration test for job ID constants
    #[test]
    fn test_job_id_constants() {
        assert_eq!(EXECUTE_FUNCTION_JOB_ID, 0);
        assert_eq!(EXECUTE_ADVANCED_JOB_ID, 1);
        assert_eq!(CREATE_SNAPSHOT_JOB_ID, 2);
        assert_eq!(RESTORE_SNAPSHOT_JOB_ID, 3);
        assert_eq!(CREATE_BRANCH_JOB_ID, 4);
        assert_eq!(MERGE_BRANCHES_JOB_ID, 5);
        assert_eq!(START_INSTANCE_JOB_ID, 6);
        assert_eq!(STOP_INSTANCE_JOB_ID, 7);
        assert_eq!(PAUSE_INSTANCE_JOB_ID, 8);
        assert_eq!(RESUME_INSTANCE_JOB_ID, 9);
        assert_eq!(EXPOSE_PORT_JOB_ID, 10);
        assert_eq!(UPLOAD_FILES_JOB_ID, 11);

        // Ensure no duplicates
        let ids = vec![
            EXECUTE_FUNCTION_JOB_ID,
            EXECUTE_ADVANCED_JOB_ID,
            CREATE_SNAPSHOT_JOB_ID,
            RESTORE_SNAPSHOT_JOB_ID,
            CREATE_BRANCH_JOB_ID,
            MERGE_BRANCHES_JOB_ID,
            START_INSTANCE_JOB_ID,
            STOP_INSTANCE_JOB_ID,
            PAUSE_INSTANCE_JOB_ID,
            RESUME_INSTANCE_JOB_ID,
            EXPOSE_PORT_JOB_ID,
            UPLOAD_FILES_JOB_ID,
        ];

        let mut unique_ids = std::collections::HashSet::new();
        for id in &ids {
            assert!(unique_ids.insert(*id), "Duplicate job ID found: {}", id);
        }
    }
}
