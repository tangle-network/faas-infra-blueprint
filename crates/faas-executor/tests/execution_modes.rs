//! Integration coverage for each executor mode without mocks.
//! These tests launch real Docker containers (or Firecracker if available) to
//! guarantee the `Executor` works out-of-the-box before blueprint orchestration.

use anyhow::Result;
use faas_common::Runtime;
use faas_executor::platform::executor::{Executor, Mode, Request};
use faas_executor::test_utils;
use serial_test::serial;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const TEST_IMAGE: &str = "alpine:latest";

fn docker_available() -> bool {
    static DOCKER_AVAILABLE: OnceLock<bool> = OnceLock::new();
    let available = *DOCKER_AVAILABLE.get_or_init(test_utils::has_docker);
    if !available {
        eprintln!("Test skipped: Docker not available");
    }
    available
}

async fn new_executor() -> Result<Executor> {
    // Disable prewarming for faster, deterministic test setup.
    std::env::set_var("FAAS_DISABLE_PREWARM", "1");
    Executor::new().await
}

fn basic_request(id: &str, code: &str, mode: Mode) -> Request {
    Request {
        id: id.to_string(),
        code: code.to_string(),
        mode,
        env: TEST_IMAGE.to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
        runtime: None,
        env_vars: None,
    }
}

#[tokio::test]
#[serial]
async fn executor_runs_ephemeral_mode() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let req = basic_request(
        "mode-ephemeral",
        r#"echo "ephemeral hello" && uname -a"#,
        Mode::Ephemeral,
    );

    let response = executor.run(req).await?;
    assert_eq!(response.exit_code, 0);
    let output = String::from_utf8_lossy(&response.stdout);
    assert!(
        output.contains("ephemeral hello"),
        "expected ephemeral output, got {output}"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_ephemeral_mode_respects_env_vars() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let mut req = basic_request(
        "mode-ephemeral-env",
        r#"
            if [ "$FAAS_EXPORTED_FLAG" = "env-works" ]; then
                echo "env ok"
            else
                echo "env missing"
                exit 17
            fi
        "#,
        Mode::Ephemeral,
    );
    let mut env = HashMap::new();
    env.insert("FAAS_EXPORTED_FLAG".to_string(), "env-works".to_string());
    req.env_vars = Some(env);

    let response = executor.run(req).await?;
    assert_eq!(response.exit_code, 0);
    let output = String::from_utf8_lossy(&response.stdout);
    assert!(
        output.contains("env ok"),
        "expected env export, saw {output}"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_runs_cached_mode_with_cache_hit() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;

    let code = r#"
            echo "Calculating factorial"
            result=1
            for i in $(seq 1 10); do
                result=$((result * i))
            done
            echo "Result: $result"
            >&2 echo "Cached execution finished"
        "#;

    let req = basic_request("mode-cached-initial", code, Mode::Cached);

    let start = Instant::now();
    let first = executor.run(req).await?;
    let first_duration = start.elapsed();

    assert_eq!(first.exit_code, 0);
    let output_first = String::from_utf8_lossy(&first.stdout);
    assert!(output_first.contains("Result: 3628800"));
    assert!(
        !first.stderr.is_empty(),
        "expected container logs on cold run"
    );

    // Trigger cache hit by reusing identical code/env.
    let start_hit = Instant::now();
    let second = executor
        .run(basic_request("mode-cached-hit", code, Mode::Cached))
        .await?;
    let hit_duration = start_hit.elapsed();

    assert_eq!(second.exit_code, 0);
    assert_eq!(first.stdout, second.stdout);
    assert!(
        second.stderr.is_empty(),
        "cached response should not include container logs"
    );
    assert!(
        hit_duration < first_duration,
        "cache hit should be faster: first {:?}, hit {:?}",
        first_duration,
        hit_duration
    );
    assert!(
        hit_duration < Duration::from_millis(200),
        "cache hit should be near-instant, observed {:?}",
        hit_duration
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_cached_mode_miss_after_code_change() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let base_code = r#"
            echo "Caching demonstration"
            echo "payload: 123"
            >&2 echo "cache log stream"
        "#;

    let first = executor
        .run(basic_request("mode-cached-base", base_code, Mode::Cached))
        .await?;
    assert_eq!(first.exit_code, 0);

    let mutated_code = base_code.replace("payload: 123", "payload: 456");
    let miss = executor
        .run(basic_request(
            "mode-cached-miss",
            &mutated_code,
            Mode::Cached,
        ))
        .await?;
    assert_eq!(miss.exit_code, 0);
    assert_ne!(
        first.stdout, miss.stdout,
        "modified code should invalidate cached payload"
    );

    let hit = executor
        .run(basic_request(
            "mode-cached-miss-hit",
            &mutated_code,
            Mode::Cached,
        ))
        .await?;
    assert_eq!(hit.exit_code, 0);
    assert_eq!(miss.stdout, hit.stdout);
    assert!(
        hit.stderr.is_empty(),
        "cached response for mutated code should be log-free"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_cached_mode_failure_not_cached() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let code = r#"
            echo "attempt $(date +%s%N)" >&2
            exit 9
        "#;

    let first = executor
        .run(basic_request("mode-cached-failure", code, Mode::Cached))
        .await?;
    assert_eq!(first.exit_code, 1, "failure should surface as exit 1");
    let first_logs = String::from_utf8_lossy(&first.stderr).to_string();
    assert!(
        first_logs.contains("attempt"),
        "expected stderr to include attempt marker: {first_logs}"
    );

    let second = executor
        .run(basic_request("mode-cached-failure", code, Mode::Cached))
        .await?;
    assert_eq!(second.exit_code, 1);
    let second_logs = String::from_utf8_lossy(&second.stderr).to_string();
    assert_ne!(
        first_logs, second_logs,
        "failed executions must not be cached"
    );

    Ok(())
}

#[cfg_attr(
    not(feature = "checkpoint-tests"),
    ignore = "requires CRIU snapshot support"
)]
#[tokio::test]
#[serial]
async fn executor_runs_checkpointed_mode() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;

    // First run generates a snapshot for the function id.
    let create_req = basic_request("mode-checkpointed", "", Mode::Checkpointed);
    let created = executor.run(create_req).await?;
    assert_eq!(created.exit_code, 0);
    assert_eq!(created.stdout, b"Checkpointed");
    let snapshot_id = created
        .snapshot
        .clone()
        .expect("checkpoint run should return snapshot id");

    // Second run restores from the previously created snapshot.
    let mut restore_req = basic_request("mode-checkpointed-restore", "", Mode::Checkpointed);
    restore_req.checkpoint = Some(snapshot_id.clone());
    let restored = executor.run(restore_req).await?;
    assert_eq!(restored.exit_code, 0);
    assert_eq!(restored.stdout, b"Restored");
    assert_eq!(restored.snapshot, Some(snapshot_id));

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_checkpointed_mode_errors_on_missing_snapshot() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let mut req = basic_request("mode-checkpoint-missing", "", Mode::Checkpointed);
    req.checkpoint = Some("nonexistent-snapshot".to_string());

    let err = executor
        .run(req)
        .await
        .expect_err("restoring unknown snapshot must error");
    assert!(
        err.to_string().contains("Snapshot not found"),
        "unexpected error when restoring missing snapshot: {err}"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_runs_branched_mode() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let mut req = basic_request("mode-branched", r#"echo "branch output""#, Mode::Branched);
    req.branch_from = Some("parent-branch".to_string());

    let response = executor.run(req).await?;
    assert_eq!(response.exit_code, 0);
    let output = String::from_utf8_lossy(&response.stdout);
    assert!(
        output.contains("branch output"),
        "expected branched output, got {output}"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_rejects_branch_without_parent() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let req = basic_request(
        "mode-branch-missing-parent",
        r#"echo "should fail""#,
        Mode::Branched,
    );

    let err = executor
        .run(req)
        .await
        .expect_err("branching without parent must error");
    assert!(
        err.to_string().contains("branch_from required"),
        "unexpected error when branch parent missing: {err}"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_runs_persistent_mode() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let req = basic_request(
        "mode-persistent",
        r#"echo "persistent state maintained""#,
        Mode::Persistent,
    );

    let first = executor.run(req).await?;
    assert_eq!(first.exit_code, 0);
    let first_output = String::from_utf8_lossy(&first.stdout);
    assert!(
        first_output.contains("persistent state maintained"),
        "expected persistent output, got {first_output}"
    );

    // Running again with the same function id should execute fresh code rather than returning a cache hit.
    let follow_up = basic_request(
        "mode-persistent",
        r#"echo "persistent follow-up""#,
        Mode::Persistent,
    );
    let second = executor.run(follow_up).await?;
    assert_eq!(second.exit_code, 0);
    let second_output = String::from_utf8_lossy(&second.stdout);
    assert!(
        second_output.contains("persistent follow-up"),
        "expected follow-up execution output, got {second_output}"
    );

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tokio::test]
#[serial]
async fn executor_persistent_firecracker_runtime_unavailable() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let mut req = basic_request(
        "mode-persistent-firecracker-request",
        r#"echo "should not reach firecracker""#,
        Mode::Persistent,
    );
    req.runtime = Some(Runtime::Firecracker);

    let err = executor
        .run(req)
        .await
        .expect_err("Firecracker runtime should fail on non-Linux hosts");
    assert!(
        err.to_string().contains("KVM not available"),
        "unexpected error for missing Firecracker: {err}"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_persistent_mode_preserves_workspace() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let persist_root_raw = std::env::current_dir()?.join("target/faas-persistent-tests");
    std::fs::create_dir_all(&persist_root_raw)?;
    let persist_root = persist_root_raw
        .canonicalize()
        .unwrap_or(persist_root_raw.clone());
    let previous = std::env::var("FAAS_PERSIST_ROOT").ok();
    std::env::set_var(
        "FAAS_PERSIST_ROOT",
        persist_root.to_string_lossy().to_string(),
    );
    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(prev) = &self.prev {
                std::env::set_var(self.key, prev);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
    let _guard = EnvGuard {
        key: "FAAS_PERSIST_ROOT",
        prev: previous,
    };

    let executor = new_executor().await?;
    let function_id = "mode-persistent-workspace";

    let first_code = r#"
            set -e
            echo "first run" >> /workspace/state.log
            cat /workspace/state.log
        "#;
    let second_code = r#"
            set -e
            echo "second run" >> /workspace/state.log
            cat /workspace/state.log
        "#;

    let first = executor
        .run(basic_request(function_id, first_code, Mode::Persistent))
        .await?;
    assert_eq!(first.exit_code, 0);
    assert!(
        String::from_utf8_lossy(&first.stdout).contains("first run"),
        "expected workspace to contain initial entry"
    );

    let workspace_file = persist_root.join(function_id).join("state.log");
    assert!(
        workspace_file.exists(),
        "workspace file {:?} should exist after first run",
        workspace_file
    );
    let host_contents = std::fs::read_to_string(&workspace_file)?;
    assert!(
        host_contents.contains("first run"),
        "host workspace should persist first entry: {host_contents}"
    );

    let second = executor
        .run(basic_request(function_id, second_code, Mode::Persistent))
        .await?;
    assert_eq!(second.exit_code, 0);
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(
        second_stdout.contains("first run"),
        "container stdout should include persisted content: {second_stdout}"
    );
    let combined_contents = std::fs::read_to_string(&workspace_file)?;
    assert!(
        combined_contents.contains("first run") && combined_contents.contains("second run"),
        "workspace should retain both entries, contents: {combined_contents}"
    );

    std::fs::remove_dir_all(&persist_root)?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_propagates_non_zero_exit() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let req = basic_request(
        "mode-non-zero",
        r#"echo "fatal error" >&2; exit 17"#,
        Mode::Ephemeral,
    );

    let response = executor.run(req).await?;
    let stderr = String::from_utf8_lossy(&response.stderr);
    assert!(
        stderr.contains("fatal error"),
        "stderr should surface container stderr, got {stderr}"
    );
    assert!(
        response.stdout.is_empty(),
        "non-zero exit should not cache stdout payload"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn executor_reports_container_errors() -> Result<()> {
    if !docker_available() {
        return Ok(());
    }

    let executor = new_executor().await?;
    let mut req = basic_request("mode-error", "echo boom", Mode::Ephemeral);
    req.env = "docker.io/library/image-that-should-not-exist:latest".to_string();

    let result = executor.run(req).await;
    assert!(result.is_err(), "expected container creation failure");

    Ok(())
}

#[cfg(all(feature = "firecracker-tests", target_os = "linux"))]
mod firecracker_paths {
    use super::*;
    use faas_common::Runtime;

    #[tokio::test]
    #[serial]
    async fn executor_runs_persistent_with_firecracker_runtime() -> Result<()> {
        if !super::test_utils::has_firecracker() {
            eprintln!("Test skipped: Firecracker binary not available");
            return Ok(());
        }

        let executor = new_executor().await?;
        let mut req = basic_request(
            "mode-persistent-firecracker",
            r#"echo "firecracker persistent""#,
            Mode::Persistent,
        );
        req.runtime = Some(Runtime::Firecracker);

        let response = executor.run(req).await?;
        assert_eq!(response.exit_code, 0);
        let output = String::from_utf8_lossy(&response.stdout);
        assert!(
            output.contains("firecracker persistent"),
            "expected Firecracker-backed output, got {output}"
        );

        Ok(())
    }
}
