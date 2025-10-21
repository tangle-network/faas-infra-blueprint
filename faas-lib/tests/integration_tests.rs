#[cfg(test)]
mod tests {
    use blueprint_sdk::extract::Context;
    use blueprint_sdk::runner::config::BlueprintEnvironment;
    use blueprint_sdk::tangle::extract::{
        CallId, TangleArg, TangleArgs2, TangleArgs3, TangleArgs4, TangleArgs6, TangleArgs8,
    };
    use faas_blueprint_lib::api_server::{ApiKeyPermissions, ApiServerConfig};
    use faas_blueprint_lib::context::FaaSContext;
    use faas_blueprint_lib::jobs::*;
    use std::fs;
    use tempfile::tempdir;
    use tokio;

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
            .expect("failed to initialize test context")
    }

    // Test all 12 Tangle jobs
    #[tokio::test]
    async fn test_execute_function_job() {
        let ctx = create_test_context().await;
        let result = execute_function_job(
            Context(ctx),
            CallId(1),
            TangleArgs4(
                "alpine:latest".to_string(),
                vec!["echo".to_string(), "test".to_string()],
                None,
                vec![],
            ),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_advanced_job() {
        let ctx = create_test_context().await;
        let result = execute_advanced_job(
            Context(ctx),
            CallId(2),
            TangleArgs8(
                "alpine:latest".to_string(),
                vec!["echo".to_string(), "test".to_string()],
                None,
                vec![],
                "ephemeral".to_string(),
                None,
                None,
                Some(60),
            ),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_snapshot_job() {
        let ctx = create_test_context().await;
        let args = CreateSnapshotArgs {
            container_id: "test-container".to_string(),
            name: "test-snapshot".to_string(),
            description: Some("metadata".to_string()),
        };

        let snapshot_id = create_snapshot_job(
            Context(ctx.clone()),
            CallId(3),
            TangleArgs3(
                args.container_id.clone(),
                args.name.clone(),
                args.description.clone(),
            ),
        )
        .await
        .expect("snapshot creation should succeed")
        .0;

        let metadata_path = ctx
            .config
            .data_dir
            .join("snapshots")
            .join(format!("{}.json", snapshot_id));
        let data = tokio::fs::read_to_string(metadata_path)
            .await
            .expect("snapshot metadata missing");
        let json: serde_json::Value = serde_json::from_str(&data).expect("invalid metadata json");
        assert_eq!(json["container_id"], "test-container");
        assert_eq!(json["description"], "metadata");
    }

    #[tokio::test]
    async fn test_restore_snapshot_job() {
        let ctx = create_test_context().await;
        let snapshot_id = create_snapshot_job(
            Context(ctx.clone()),
            CallId(20),
            TangleArgs3("restore-source".to_string(), "restore".to_string(), None),
        )
        .await
        .expect("snapshot creation failed")
        .0;

        let container_id = restore_snapshot_job(
            Context(ctx.clone()),
            CallId(4),
            TangleArg(snapshot_id.clone()),
        )
        .await
        .expect("restore should succeed")
        .0;

        let metadata_path = ctx
            .config
            .data_dir
            .join("instances")
            .join(format!("{}.json", container_id));
        let data = tokio::fs::read_to_string(metadata_path)
            .await
            .expect("restored instance metadata missing");
        let json: serde_json::Value = serde_json::from_str(&data).expect("invalid metadata json");
        assert_eq!(json["snapshot_id"], snapshot_id);
        assert_eq!(json["status"], "Running");
    }

    #[tokio::test]
    async fn test_create_branch_job() {
        let ctx = create_test_context().await;
        let snapshot_id = create_snapshot_job(
            Context(ctx.clone()),
            CallId(21),
            TangleArgs3("branch-source".to_string(), "branch".to_string(), None),
        )
        .await
        .expect("snapshot creation failed")
        .0;

        let branch_id = create_branch_job(
            Context(ctx.clone()),
            CallId(5),
            TangleArgs2(snapshot_id.clone(), "test-branch".to_string()),
        )
        .await
        .expect("branch creation should succeed")
        .0;

        let metadata_path = ctx
            .config
            .data_dir
            .join("branches")
            .join(format!("{}.json", branch_id));
        let data = tokio::fs::read_to_string(metadata_path)
            .await
            .expect("branch metadata missing");
        let json: serde_json::Value = serde_json::from_str(&data).expect("invalid metadata json");
        assert_eq!(json["parent_snapshot_id"], snapshot_id);
    }

    #[tokio::test]
    async fn test_merge_branches_job() {
        let ctx = create_test_context().await;
        let snapshot_id = create_snapshot_job(
            Context(ctx.clone()),
            CallId(22),
            TangleArgs3("merge".to_string(), "base".to_string(), None),
        )
        .await
        .expect("snapshot creation failed")
        .0;

        let branch_a = create_branch_job(
            Context(ctx.clone()),
            CallId(23),
            TangleArgs2(snapshot_id.clone(), "branch-a".to_string()),
        )
        .await
        .expect("branch creation failed")
        .0;
        let branch_b = create_branch_job(
            Context(ctx.clone()),
            CallId(24),
            TangleArgs2(snapshot_id.clone(), "branch-b".to_string()),
        )
        .await
        .expect("branch creation failed")
        .0;

        let merged = merge_branches_job(
            Context(ctx.clone()),
            CallId(6),
            TangleArgs2(
                vec![branch_a.clone(), branch_b.clone()],
                "latest".to_string(),
            ),
        )
        .await
        .expect("merge should succeed")
        .0;

        let metadata_path = ctx
            .config
            .data_dir
            .join("branches")
            .join(format!("{}.json", merged));
        let data = tokio::fs::read_to_string(metadata_path)
            .await
            .expect("merged metadata missing");
        let json: serde_json::Value = serde_json::from_str(&data).expect("invalid metadata json");
        let parents = json["parent_snapshot_id"].as_str().unwrap();
        assert!(parents.contains(&branch_a));
        assert!(parents.contains(&branch_b));
    }

    #[tokio::test]
    async fn test_start_instance_job() {
        let ctx = create_test_context().await;
        let args = StartInstanceArgs {
            snapshot_id: None,
            image: Some("alpine:latest".to_string()),
            cpu_cores: 1,
            memory_mb: 512,
            disk_gb: 1,
            enable_ssh: false,
        };

        let instance_id = start_instance_job(
            Context(ctx.clone()),
            CallId(7),
            TangleArgs6(
                args.snapshot_id.clone(),
                args.image.clone(),
                args.cpu_cores,
                args.memory_mb,
                args.disk_gb,
                args.enable_ssh,
            ),
        )
        .await
        .expect("start instance should succeed")
        .0;

        let metadata_path = ctx
            .config
            .data_dir
            .join("instances")
            .join(format!("{}.json", instance_id));
        let data = tokio::fs::read_to_string(metadata_path)
            .await
            .expect("instance metadata missing");
        let json: serde_json::Value = serde_json::from_str(&data).expect("invalid metadata json");
        assert_eq!(json["status"], "Running");
        assert_eq!(json["cpu_cores"], 1);
    }

    #[tokio::test]
    async fn test_stop_instance_job() {
        let ctx = create_test_context().await;
        let instance_id = start_instance_job(
            Context(ctx.clone()),
            CallId(30),
            TangleArgs6(None, Some("alpine:latest".to_string()), 1, 256, 1, false),
        )
        .await
        .expect("start instance failed")
        .0;

        stop_instance_job(
            Context(ctx.clone()),
            CallId(8),
            TangleArg(instance_id.clone()),
        )
        .await
        .expect("stop instance should succeed");

        let metadata_path = ctx
            .config
            .data_dir
            .join("instances")
            .join(format!("{}.json", instance_id));
        let data = tokio::fs::read_to_string(metadata_path)
            .await
            .expect("stopped instance metadata missing");
        let json: serde_json::Value = serde_json::from_str(&data).expect("invalid metadata json");
        assert_eq!(json["status"], "Stopped");
    }

    #[tokio::test]
    async fn test_pause_instance_job() {
        let ctx = create_test_context().await;
        let instance_id = start_instance_job(
            Context(ctx.clone()),
            CallId(31),
            TangleArgs6(None, Some("alpine:latest".to_string()), 1, 256, 1, false),
        )
        .await
        .expect("start instance failed")
        .0;

        let checkpoint_id = pause_instance_job(
            Context(ctx.clone()),
            CallId(9),
            TangleArg(instance_id.clone()),
        )
        .await
        .expect("pause instance should succeed")
        .0;

        let checkpoint_path = ctx
            .config
            .data_dir
            .join("checkpoints")
            .join(format!("{}.json", checkpoint_id));
        assert!(checkpoint_path.exists(), "checkpoint file missing");

        let instance_path = ctx
            .config
            .data_dir
            .join("instances")
            .join(format!("{}.json", instance_id));
        let instance_data = tokio::fs::read_to_string(instance_path)
            .await
            .expect("paused instance metadata missing");
        let json: serde_json::Value =
            serde_json::from_str(&instance_data).expect("invalid metadata json");
        assert_eq!(json["status"], "Paused");
    }

    #[tokio::test]
    async fn test_resume_instance_job() {
        let ctx = create_test_context().await;
        let instance_id = start_instance_job(
            Context(ctx.clone()),
            CallId(32),
            TangleArgs6(None, Some("alpine:latest".to_string()), 1, 256, 1, false),
        )
        .await
        .expect("start instance failed")
        .0;

        let checkpoint_id = pause_instance_job(
            Context(ctx.clone()),
            CallId(33),
            TangleArg(instance_id.clone()),
        )
        .await
        .expect("pause instance failed")
        .0;

        let resumed = resume_instance_job(
            Context(ctx.clone()),
            CallId(10),
            TangleArg(checkpoint_id.clone()),
        )
        .await
        .expect("resume instance should succeed")
        .0;

        assert_eq!(resumed, "resumed_10".to_string());

        let instance_path = ctx
            .config
            .data_dir
            .join("instances")
            .join(format!("{}.json", instance_id));
        let data = tokio::fs::read_to_string(instance_path)
            .await
            .expect("resumed instance metadata missing");
        let json: serde_json::Value = serde_json::from_str(&data).expect("invalid metadata json");
        assert_eq!(json["status"], "Running");

        let checkpoint_path = ctx
            .config
            .data_dir
            .join("checkpoints")
            .join(format!("{}.json", checkpoint_id));
        assert!(
            !checkpoint_path.exists(),
            "checkpoint file should be removed after resume"
        );
    }

    #[tokio::test]
    async fn test_expose_port_job() {
        let ctx = create_test_context().await;
        let instance_id = start_instance_job(
            Context(ctx.clone()),
            CallId(34),
            TangleArgs6(None, Some("alpine:latest".to_string()), 1, 256, 1, false),
        )
        .await
        .expect("start instance failed")
        .0;

        let url = expose_port_job(
            Context(ctx.clone()),
            CallId(11),
            TangleArgs4(
                instance_id.clone(),
                8080,
                "http".to_string(),
                Some("myapp".to_string()),
            ),
        )
        .await
        .expect("expose port should succeed")
        .0;

        assert!(url.contains("myapp.faas.local"));

        let exposure_path = ctx
            .config
            .data_dir
            .join("exposures")
            .join(format!("{}_11.json", instance_id));
        let data = tokio::fs::read_to_string(exposure_path)
            .await
            .expect("exposure metadata missing");
        let json: serde_json::Value = serde_json::from_str(&data).expect("invalid metadata json");
        assert_eq!(json["url"], url);
    }

    #[tokio::test]
    async fn test_upload_files_job() {
        let ctx = create_test_context().await;
        let instance_id = start_instance_job(
            Context(ctx.clone()),
            CallId(35),
            TangleArgs6(None, Some("alpine:latest".to_string()), 1, 256, 1, false),
        )
        .await
        .expect("start instance failed")
        .0;

        let payload = vec![1u8, 2, 3, 4, 5];
        let bytes = upload_files_job(
            Context(ctx.clone()),
            CallId(12),
            TangleArgs3(instance_id.clone(), "data".to_string(), payload.clone()),
        )
        .await
        .expect("upload files should succeed")
        .0;

        assert_eq!(bytes, payload.len() as u64);

        let upload_path = ctx
            .config
            .data_dir
            .join("uploads")
            .join(&instance_id)
            .join("data")
            .join("call_12.bin");
        let stored = tokio::fs::read(upload_path)
            .await
            .expect("uploaded file missing");
        assert_eq!(stored, payload);
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
            let ctx = create_test_context().await;
            let _result = execute_advanced_job(
                Context(ctx),
                CallId(100),
                TangleArgs8(
                    "alpine:latest".to_string(),
                    vec!["echo".to_string(), mode_str.to_string()],
                    None,
                    vec![],
                    mode_str.to_string(),
                    None,
                    None,
                    Some(5),
                ),
            )
            .await;

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
