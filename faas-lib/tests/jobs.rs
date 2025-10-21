use blueprint_sdk::crypto::sp_core::{SpEcdsa, SpSr25519};
use blueprint_sdk::extract::Context;
use blueprint_sdk::keystore::backends::Backend;
use blueprint_sdk::keystore::{Keystore, KeystoreConfig};
use blueprint_sdk::runner::config::BlueprintEnvironment;
use blueprint_sdk::tangle::extract::{CallId, TangleArgs4, TangleArgs8};
use color_eyre::eyre::{eyre, Result};
use faas_blueprint_lib::context::FaaSContext;
use faas_blueprint_lib::jobs::{execute_advanced_job, execute_function_job};
use faas_executor::platform::{Mode, Request};
use std::fs;
use tempfile::TempDir;

struct ContextFixture {
    ctx: FaaSContext,
    _dir: TempDir,
}

async fn create_fixture() -> Result<ContextFixture> {
    std::env::set_var("FAAS_DISABLE_CONTRACT_ASSIGNMENT", "1");
    std::env::set_var("FAAS_DISABLE_PREWARM", "1");

    let temp_dir = tempfile::tempdir()?;
    let base_path = temp_dir.path().to_path_buf();
    let keystore_dir = base_path.join("keystore");
    fs::create_dir_all(&keystore_dir)?;

    let mut env = BlueprintEnvironment::default();
    env.test_mode = true;
    env.data_dir = base_path;
    env.keystore_uri = keystore_dir.to_string_lossy().into_owned();
    env.protocol_settings = blueprint_sdk::runner::config::ProtocolSettings::None;

    // Initialize keystore with required key types (sr25519 + ecdsa) so the context can derive addresses.
    let keystore_config = KeystoreConfig::new().fs_root(env.keystore_uri.clone());
    let keystore = Keystore::new(keystore_config)?;
    // Generate a deterministic sr25519 key so blueprint SDK expectations are met.
    let _ = keystore.generate::<SpSr25519>(None)?;
    // Generate matching ECDSA key for operator address derivation.
    let _ = keystore.generate::<SpEcdsa>(None)?;

    let ctx = FaaSContext::new(env)
        .await
        .map_err(|e| eyre!("failed to create FaaSContext: {e}"))?;

    verify_executor(&ctx).await?;

    Ok(ContextFixture {
        ctx,
        _dir: temp_dir,
    })
}

async fn verify_executor(ctx: &FaaSContext) -> Result<()> {
    let request = Request {
        id: "probe".to_string(),
        code: "echo probe".to_string(),
        mode: Mode::Ephemeral,
        env: "alpine:latest".to_string(),
        timeout: std::time::Duration::from_secs(10),
        checkpoint: None,
        branch_from: None,
        runtime: Some(faas_common::Runtime::Docker),
        env_vars: None,
    };

    let response = ctx
        .executor
        .run(request)
        .await
        .map_err(|e| eyre!("Executor probe failed. Ensure Docker daemon is running: {e}"))?;

    if response.exit_code != 0 {
        let stderr = String::from_utf8_lossy(&response.stderr);
        return Err(eyre!(
            "Executor probe exited with {}. stderr: {}",
            response.exit_code,
            stderr.trim()
        ));
    }

    if response.stdout.is_empty() {
        return Err(eyre!(
            "Executor probe returned empty stdout; expected real command output"
        ));
    }

    Ok(())
}

#[tokio::test]
async fn execute_function_emits_stdout() -> Result<()> {
    let fixture = create_fixture().await?;
    let ctx = fixture.ctx.clone();

    let result = execute_function_job(
        Context(ctx),
        CallId(1),
        TangleArgs4(
            "alpine:latest".to_string(),
            vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo test-output".to_string(),
            ],
            None,
            vec![],
        ),
    )
    .await?;

    let output = String::from_utf8(result.0).expect("stdout should be valid UTF-8");
    assert!(
        output.contains("test-output"),
        "expected command stdout, got {output}"
    );
    Ok(())
}

#[tokio::test]
async fn execute_advanced_honors_mode() -> Result<()> {
    let fixture = create_fixture().await?;
    let ctx = fixture.ctx.clone();

    let result = execute_advanced_job(
        Context(ctx),
        CallId(2),
        TangleArgs8(
            "alpine:latest".to_string(),
            vec!["sh".to_string(), "-c".to_string(), "echo json".to_string()],
            None,
            vec![],
            "cached".to_string(),
            None,
            None,
            Some(30),
        ),
    )
    .await?;

    let stdout = String::from_utf8(result.0).expect("stdout should be valid UTF-8");
    assert!(
        stdout.contains("json"),
        "expected cached execution output, got {stdout}"
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_job_invocations_complete() -> Result<()> {
    let fixture = create_fixture().await?;
    let ctx = fixture.ctx.clone();

    let mut tasks = Vec::new();
    for i in 0..4u64 {
        let ctx_clone = ctx.clone();
        tasks.push(tokio::spawn(async move {
            execute_function_job(
                Context(ctx_clone),
                CallId(100 + i),
                TangleArgs4(
                    "alpine:latest".to_string(),
                    vec![
                        "sh".to_string(),
                        "-c".to_string(),
                        format!("echo '{i}' && sleep 0.1"),
                    ],
                    None,
                    vec![],
                ),
            )
            .await
        }));
    }

    for task in tasks {
        let result = task.await?;
        let output = String::from_utf8(result?.0).expect("stdout should be valid UTF-8");
        assert!(
            output.trim().ends_with('0')
                || output.trim().ends_with('1')
                || output.trim().ends_with('2')
                || output.trim().ends_with('3'),
            "unexpected stdout: {output}"
        );
    }

    Ok(())
}

#[tokio::test]
async fn invalid_image_returns_error() -> Result<()> {
    let fixture = create_fixture().await?;
    let ctx = fixture.ctx.clone();

    let result = execute_function_job(
        Context(ctx),
        CallId(999),
        TangleArgs4(
            "nonexistent:image".to_string(),
            vec!["echo".to_string(), "should fail".to_string()],
            None,
            vec![],
        ),
    )
    .await;

    assert!(
        result.is_err(),
        "Expected execution to fail for nonexistent image"
    );
    Ok(())
}

#[test]
fn print_field_types() {
    use blueprint_sdk::tangle::metadata::IntoTangleFieldTypes;
    let execute_fields = faas_common::ExecuteFunctionArgs::into_tangle_fields();
    println!(
        "execute params: {}",
        blueprint_sdk::tangle::metadata::macros::ext::serde_json::to_string(&execute_fields)
            .unwrap()
    );

    let advanced_fields = faas_blueprint_lib::jobs::ExecuteAdvancedArgs::into_tangle_fields();
    println!(
        "advanced params: {}",
        blueprint_sdk::tangle::metadata::macros::ext::serde_json::to_string(&advanced_fields)
            .unwrap()
    );

    let snapshot_fields = faas_blueprint_lib::jobs::CreateSnapshotArgs::into_tangle_fields();
    println!(
        "snapshot params: {}",
        blueprint_sdk::tangle::metadata::macros::ext::serde_json::to_string(&snapshot_fields)
            .unwrap()
    );

    let start_fields = faas_blueprint_lib::jobs::StartInstanceArgs::into_tangle_fields();
    println!(
        "start params: {}",
        blueprint_sdk::tangle::metadata::macros::ext::serde_json::to_string(&start_fields).unwrap()
    );

    let expose_fields = faas_blueprint_lib::jobs::ExposePortArgs::into_tangle_fields();
    println!(
        "expose params: {}",
        blueprint_sdk::tangle::metadata::macros::ext::serde_json::to_string(&expose_fields)
            .unwrap()
    );

    let upload_fields = faas_blueprint_lib::jobs::UploadFilesArgs::into_tangle_fields();
    println!(
        "upload params: {}",
        blueprint_sdk::tangle::metadata::macros::ext::serde_json::to_string(&upload_fields)
            .unwrap()
    );
}
