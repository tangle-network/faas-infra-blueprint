// Simple smoke test to verify basic functionality without Docker
use faas_blueprint_lib::jobs::*;

#[test]
fn test_job_constants() {
    // Verify all job IDs are unique
    let job_ids = vec![
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

    for i in 0..job_ids.len() {
        for j in i + 1..job_ids.len() {
            assert_ne!(job_ids[i], job_ids[j], "Duplicate job ID found");
        }
    }

    // Verify sequential IDs
    for (i, id) in job_ids.iter().enumerate() {
        assert_eq!(*id, i as u64, "Job ID {} should be {}", id, i);
    }
}

#[test]
fn test_arg_structures_exist() {
    // Just verify the structures compile and can be created
    let _ = ExecuteAdvancedArgs {
        image: "test".to_string(),
        command: vec![],
        env_vars: None,
        payload: vec![],
        mode: "ephemeral".to_string(),
        checkpoint_id: None,
        branch_from: None,
        timeout_secs: None,
    };

    let _ = CreateSnapshotArgs {
        container_id: "test".to_string(),
        name: "test".to_string(),
        description: None,
    };

    let _ = StartInstanceArgs {
        snapshot_id: None,
        image: Some("test".to_string()),
        cpu_cores: 1,
        memory_mb: 512,
        disk_gb: 1,
        enable_ssh: false,
    };
}