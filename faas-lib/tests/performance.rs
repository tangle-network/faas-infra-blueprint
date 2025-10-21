use color_eyre::Result;
use faas_executor::platform::{Executor, Mode, Request};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::Instant;

async fn create_executor() -> Result<Executor> {
    Executor::new()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("failed to initialize executor: {e}"))
}

fn alpine_echo_request(id: &str, payload: &str, mode: Mode) -> Request {
    Request {
        id: id.to_string(),
        code: format!("echo {}", payload),
        mode,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
        runtime: Some(faas_common::Runtime::Docker),
        env_vars: None,
    }
}

async fn run(executor: &Executor, request: Request) -> Result<faas_executor::platform::Response> {
    executor
        .run(request)
        .await
        .map_err(|e| color_eyre::eyre::eyre!("executor run failed: {e}"))
}

#[tokio::test]
async fn cold_start_execution_completes() -> Result<()> {
    let executor = create_executor().await?;

    let request = alpine_echo_request("cold-start", "performance", Mode::Ephemeral);
    let response = run(&executor, request).await?;

    assert_eq!(response.exit_code, 0);
    let stdout = String::from_utf8_lossy(&response.stdout);
    assert!(
        stdout.contains("performance"),
        "expected echo output, got {stdout}"
    );
    Ok(())
}

#[tokio::test]
async fn cached_mode_is_not_slower_than_ephemeral() -> Result<()> {
    let executor = create_executor().await?;

    // Warm up ephemeral execution for baseline
    let baseline_req = alpine_echo_request("baseline", "baseline", Mode::Ephemeral);
    let baseline_start = Instant::now();
    let baseline = run(&executor, baseline_req).await?;
    assert_eq!(baseline.exit_code, 0);
    let baseline_duration = baseline_start.elapsed();

    // Cached execution should reuse layers and be comparable or faster
    let cached_req = alpine_echo_request("cached", "cached", Mode::Cached);
    let cached_start = Instant::now();
    let cached = run(&executor, cached_req).await?;
    assert_eq!(cached.exit_code, 0);
    let cached_duration = cached_start.elapsed();

    assert!(
        cached_duration <= baseline_duration * 2,
        "cached execution took {:?}, baseline {:?}",
        cached_duration,
        baseline_duration
    );
    Ok(())
}

#[tokio::test]
async fn execution_respects_env_variables() -> Result<()> {
    let executor = create_executor().await?;

    let mut env = HashMap::new();
    env.insert("PERF_ENV".to_string(), "1".to_string());

    let request = Request {
        id: "env-test".to_string(),
        code: "sh -c 'echo $PERF_ENV'".to_string(),
        mode: Mode::Ephemeral,
        env: "alpine:latest".to_string(),
        timeout: Duration::from_secs(30),
        checkpoint: None,
        branch_from: None,
        runtime: Some(faas_common::Runtime::Docker),
        env_vars: Some(env),
    };

    let response = run(&executor, request).await?;
    assert_eq!(response.exit_code, 0);
    let stdout = String::from_utf8_lossy(&response.stdout);
    assert!(
        stdout.contains('1'),
        "expected env var output, got {stdout}"
    );
    Ok(())
}
