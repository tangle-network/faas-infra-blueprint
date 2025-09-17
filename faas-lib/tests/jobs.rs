use faas_common::{ExecuteFunctionArgs, InvocationResult, SandboxExecutor};
use faas_executor::docktopus::DockerBuilder;
use faas_executor::executor::{ContainerStrategy, ExecutionStrategy};
use faas_executor::Executor;
use faas_blueprint_lib::context::FaaSContext;
use faas_blueprint_lib::jobs::execute_function_job;
use faas_blueprint_lib::{ExecuteFunctionResult, FaaSExecutionOutput, EXECUTE_FUNCTION_JOB_ID};
use faas_orchestrator::Orchestrator;
use blueprint_sdk::extract::Context;
use blueprint_sdk::tangle::extract::{CallId, TangleArg};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info};

// ==================== Test Helpers ====================

async fn create_faas_context() -> color_eyre::Result<FaaSContext> {
    let docker_builder = DockerBuilder::new().await?;
    let docker_client = docker_builder.client();

    let strategy = ExecutionStrategy::Container(ContainerStrategy {
        warm_pools: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        max_pool_size: 5,
        docker: docker_client,
        build_cache_volumes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        dependency_layers: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        gpu_pools: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    });

    let executor: Arc<dyn SandboxExecutor + Send + Sync> = Arc::new(
        Executor::new(strategy)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to create executor: {}", e))?,
    );

    let orchestrator = Arc::new(Orchestrator::new(executor));

    Ok(FaaSContext { orchestrator })
}

async fn execute_function_with_args(
    ctx: FaaSContext,
    call_id: u64,
    args: ExecuteFunctionArgs,
) -> Result<Vec<u8>, faas_blueprint_lib::JobError> {
    let result = execute_function_job(
        Context(ctx),
        CallId(call_id),
        TangleArg(args),
    )
    .await?;

    Ok(result.0)
}

// ==================== Blueprint Job Tests ====================

#[tokio::test]
async fn test_blueprint_job_simple_execution() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== BLUEPRINT JOB SIMPLE EXECUTION TEST ===");

    let ctx = create_faas_context().await?;

    let args = ExecuteFunctionArgs {
        image: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "Hello Blueprint".to_string()],
        env_vars: None,
        payload: vec![],
    };

    let result = execute_function_with_args(ctx, 1, args).await?;

    assert!(!result.is_empty(), "Should have output");
    let output = String::from_utf8_lossy(&result);
    assert!(
        output.contains("Hello Blueprint"),
        "Output should contain the echoed message"
    );

    info!("Simple blueprint job execution passed ✓");
    Ok(())
}

#[tokio::test]
async fn test_blueprint_job_with_payload() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== BLUEPRINT JOB WITH PAYLOAD TEST ===");

    let ctx = create_faas_context().await?;

    let payload_data = b"This is test payload data".to_vec();
    let args = ExecuteFunctionArgs {
        image: "alpine:latest".to_string(),
        command: vec!["cat".to_string()], // Read from stdin
        env_vars: None,
        payload: payload_data.clone(),
    };

    let result = execute_function_with_args(ctx, 2, args).await?;

    assert_eq!(
        result, payload_data,
        "Should return the payload data"
    );

    info!("Blueprint job with payload passed ✓");
    Ok(())
}

#[tokio::test]
async fn test_blueprint_job_with_env_vars() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== BLUEPRINT JOB WITH ENV VARS TEST ===");

    let ctx = create_faas_context().await?;

    let env_vars = vec![
        ("BLUEPRINT_VAR".to_string(), "blueprint_value".to_string()),
        ("TEST_MODE".to_string(), "true".to_string()),
    ];

    let args = ExecuteFunctionArgs {
        image: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo $BLUEPRINT_VAR:$TEST_MODE".to_string(),
        ],
        env_vars: Some(env_vars),
        payload: vec![],
    };

    let result = execute_function_with_args(ctx, 3, args).await?;

    let output = String::from_utf8_lossy(&result);
    assert!(
        output.contains("blueprint_value:true"),
        "Should see environment variables in output"
    );

    info!("Blueprint job with env vars passed ✓");
    Ok(())
}

#[tokio::test]
async fn test_blueprint_job_concurrent_executions() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== BLUEPRINT JOB CONCURRENT EXECUTIONS TEST ===");

    let ctx = Arc::new(create_faas_context().await?);
    let num_concurrent = 10;
    let mut handles = Vec::new();

    let start = Instant::now();

    for i in 0..num_concurrent {
        let ctx_clone = ctx.clone();
        let call_id = 100 + i;

        let handle = tokio::spawn(async move {
            let args = ExecuteFunctionArgs {
                image: "alpine:latest".to_string(),
                command: vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    format!("echo 'Job {}' && sleep 0.1", i),
                ],
                env_vars: None,
                payload: vec![],
            };

            let task_start = Instant::now();
            let result = execute_function_with_args((*ctx_clone).clone(), call_id, args).await;
            let duration = task_start.elapsed();

            (i, result, duration)
        });

        handles.push(handle);
    }

    // Collect results
    let mut successful = 0;
    let mut failed = 0;

    for handle in handles {
        match handle.await {
            Ok((i, result, duration)) => {
                match result {
                    Ok(output) => {
                        let output_str = String::from_utf8_lossy(&output);
                        assert!(
                            output_str.contains(&format!("Job {}", i)),
                            "Each job should have unique output"
                        );
                        successful += 1;
                        info!("Job {} completed in {:?}", i, duration);
                    }
                    Err(e) => {
                        error!("Job {} failed: {}", i, e);
                        failed += 1;
                    }
                }
            }
            Err(e) => {
                error!("Job join failed: {}", e);
                failed += 1;
            }
        }
    }

    let total_duration = start.elapsed();

    info!("=== CONCURRENT JOB RESULTS ===");
    info!("Successful: {}/{}", successful, num_concurrent);
    info!("Failed: {}", failed);
    info!("Total duration: {:?}", total_duration);

    assert!(
        successful >= num_concurrent * 90 / 100,
        "At least 90% of jobs should succeed"
    );

    info!("Concurrent blueprint jobs passed ✓");
    Ok(())
}

#[tokio::test]
async fn test_blueprint_job_error_handling() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== BLUEPRINT JOB ERROR HANDLING TEST ===");

    let ctx = create_faas_context().await?;

    // Test 1: Invalid command
    info!("Test 1: Invalid command");
    let invalid_cmd_args = ExecuteFunctionArgs {
        image: "alpine:latest".to_string(),
        command: vec!["/nonexistent/command".to_string()],
        env_vars: None,
        payload: vec![],
    };

    let result = execute_function_with_args(ctx.clone(), 200, invalid_cmd_args).await;

    // Should either return error in output or fail
    match result {
        Ok(output) => {
            // If it returns output, it might be an error message
            let output_str = String::from_utf8_lossy(&output);
            info!("Invalid command output: {}", output_str);
        }
        Err(e) => {
            info!("Invalid command correctly failed: {}", e);
        }
    }

    // Test 2: Non-existent image
    info!("Test 2: Non-existent image");
    let bad_image_args = ExecuteFunctionArgs {
        image: "nonexistent:image:v999".to_string(),
        command: vec!["echo".to_string(), "test".to_string()],
        env_vars: None,
        payload: vec![],
    };

    let bad_image_result = execute_function_with_args(ctx.clone(), 201, bad_image_args).await;
    assert!(
        bad_image_result.is_err(),
        "Should fail with non-existent image"
    );

    info!("Blueprint job error handling passed ✓");
    Ok(())
}

#[tokio::test]
async fn test_blueprint_job_output_conversion() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== BLUEPRINT JOB OUTPUT CONVERSION TEST ===");

    // Test InvocationResult to FaaSExecutionOutput conversion
    let invocation_result = InvocationResult {
        request_id: "test-123".to_string(),
        response: Some(b"Test output".to_vec()),
        logs: Some("Test logs".to_string()),
        error: None,
    };

    let faas_output = FaaSExecutionOutput::from(invocation_result.clone());

    assert_eq!(faas_output.request_id, "test-123");
    assert_eq!(faas_output.stdout, Some("Test output".to_string()));
    assert_eq!(faas_output.stderr, Some("Test logs".to_string()));
    assert_eq!(faas_output.error, None);

    // Test with error
    let error_result = InvocationResult {
        request_id: "error-456".to_string(),
        response: None,
        logs: Some("Error logs".to_string()),
        error: Some("Execution failed".to_string()),
    };

    let error_output = FaaSExecutionOutput::from(error_result);

    assert_eq!(error_output.request_id, "error-456");
    assert_eq!(error_output.stdout, None);
    assert_eq!(error_output.stderr, Some("Error logs".to_string()));
    assert_eq!(error_output.error, Some("Execution failed".to_string()));

    // Test ExecuteFunctionResult
    let ok_result = ExecuteFunctionResult::ok(faas_output.clone());
    assert!(matches!(ok_result, ExecuteFunctionResult::Ok(_)));

    let err_result = ExecuteFunctionResult::err("Test error".to_string());
    assert!(matches!(err_result, ExecuteFunctionResult::Err(_)));

    info!("Output conversion test passed ✓");
    Ok(())
}

#[tokio::test]
async fn test_blueprint_job_complex_workflows() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== BLUEPRINT JOB COMPLEX WORKFLOWS TEST ===");

    let ctx = create_faas_context().await?;

    // Test 1: Multi-step pipeline
    info!("Test 1: Multi-step pipeline");

    // Step 1: Generate data
    let generate_args = ExecuteFunctionArgs {
        image: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo '1,2,3,4,5'".to_string(),
        ],
        env_vars: None,
        payload: vec![],
    };

    let generated_data = execute_function_with_args(ctx.clone(), 300, generate_args).await?;

    // Step 2: Process data
    let process_args = ExecuteFunctionArgs {
        image: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat | tr ',' ' '".to_string(),
        ],
        env_vars: None,
        payload: generated_data.clone(),
    };

    let processed_data = execute_function_with_args(ctx.clone(), 301, process_args).await?;

    let processed_str = String::from_utf8_lossy(&processed_data);
    assert!(
        processed_str.contains("1 2 3 4 5"),
        "Data should be processed correctly"
    );

    // Test 2: Conditional execution based on output
    info!("Test 2: Conditional execution");

    let check_args = ExecuteFunctionArgs {
        image: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            "if [ $(date +%S) -gt 30 ]; then echo 'HIGH'; else echo 'LOW'; fi".to_string(),
        ],
        env_vars: None,
        payload: vec![],
    };

    let check_result = execute_function_with_args(ctx.clone(), 302, check_args).await?;
    let check_output = String::from_utf8_lossy(&check_result);

    // Execute different command based on result
    let followup_cmd = if check_output.contains("HIGH") {
        "echo 'Processing HIGH value'"
    } else {
        "echo 'Processing LOW value'"
    };

    let followup_args = ExecuteFunctionArgs {
        image: "alpine:latest".to_string(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            followup_cmd.to_string(),
        ],
        env_vars: None,
        payload: vec![],
    };

    let followup_result = execute_function_with_args(ctx, 303, followup_args).await?;
    let followup_output = String::from_utf8_lossy(&followup_result);
    assert!(
        followup_output.contains("Processing"),
        "Should execute conditional followup"
    );

    info!("Complex workflows test passed ✓");
    Ok(())
}

#[tokio::test]
async fn test_blueprint_job_performance_metrics() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    info!("=== BLUEPRINT JOB PERFORMANCE METRICS TEST ===");

    let ctx = create_faas_context().await?;

    let mut execution_times = Vec::new();
    let num_iterations = 10;

    // Warm-up execution
    let warmup_args = ExecuteFunctionArgs {
        image: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "warmup".to_string()],
        env_vars: None,
        payload: vec![],
    };
    let _ = execute_function_with_args(ctx.clone(), 400, warmup_args).await;

    // Measure execution times
    for i in 0..num_iterations {
        let args = ExecuteFunctionArgs {
            image: "alpine:latest".to_string(),
            command: vec![
                "echo".to_string(),
                format!("Iteration {}", i),
            ],
            env_vars: None,
            payload: vec![],
        };

        let start = Instant::now();
        let result = execute_function_with_args(ctx.clone(), 401 + i, args).await;
        let duration = start.elapsed();

        assert!(result.is_ok(), "Execution should succeed");
        execution_times.push(duration);
    }

    // Calculate metrics
    let total_time: Duration = execution_times.iter().sum();
    let avg_time = total_time / num_iterations as u32;
    let min_time = execution_times.iter().min().unwrap();
    let max_time = execution_times.iter().max().unwrap();

    info!("=== PERFORMANCE METRICS ===");
    info!("Average execution time: {:?}", avg_time);
    info!("Min execution time: {:?}", min_time);
    info!("Max execution time: {:?}", max_time);
    info!("Total time for {} executions: {:?}", num_iterations, total_time);

    // Performance assertions
    assert!(
        avg_time < Duration::from_millis(500),
        "Average execution should be under 500ms for warm containers"
    );

    assert!(
        *min_time < Duration::from_millis(100),
        "Best case should be under 100ms"
    );

    let consistency = (max_time.as_millis() - min_time.as_millis()) as f64
        / avg_time.as_millis() as f64;

    info!("Execution time consistency: {:.2}", consistency);
    assert!(
        consistency < 2.0,
        "Execution times should be relatively consistent"
    );

    info!("Performance metrics test passed ✓");
    Ok(())
}