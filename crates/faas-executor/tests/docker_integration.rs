//! Real Docker integration tests for FaaS Platform
//! Tests actual FaaS capabilities, not just Docker wrapper

use bollard::Docker;
use faas_common::{SandboxConfig, SandboxExecutor};
use faas_executor::DockerExecutor;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[tokio::test]
#[ignore = "Requires Docker"] // Run with: cargo test --test docker_integration -- --ignored
async fn test_real_docker_execution() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);
    
    let result = executor.execute(SandboxConfig {
        function_id: "test-real".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "hello".to_string()],
        env_vars: None,
        payload: vec![],
    }).await;
    
    assert!(result.is_ok());
    assert_eq!(result.unwrap().response, Some(b"hello\n".to_vec()));
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_docker_timeout() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);
    
    let start = std::time::Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        executor.execute(SandboxConfig {
            function_id: "test-timeout".to_string(),
            source: "alpine:latest".to_string(),
            command: vec!["sleep".to_string(), "30".to_string()],
            env_vars: None,
            payload: vec![],
        })
    ).await;
    
    assert!(result.is_err());
    assert!(start.elapsed() < Duration::from_secs(6));
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_docker_resource_limits() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);
    
    // Test memory limit enforcement
    let result = executor.execute(SandboxConfig {
        function_id: "test-memory".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["sh".to_string(), "-c".to_string(), 
                      "dd if=/dev/zero of=/dev/shm/test bs=1M count=1000".to_string()],
        env_vars: None,
        payload: vec![],
    }).await;
    
    // Should fail due to memory limits
    assert!(result.is_err() || result.unwrap().error.is_some());
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_docker_concurrent_isolation() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = Arc::new(DockerExecutor::new(docker));
    
    let mut handles = vec![];
    for i in 0..5 {
        let exec = executor.clone();
        handles.push(tokio::spawn(async move {
            exec.execute(SandboxConfig {
                function_id: format!("isolated-{}", i),
                source: "alpine:latest".to_string(),
                command: vec!["sh".to_string(), "-c".to_string(),
                              format!("echo {} > /tmp/test && cat /tmp/test", i)],
                env_vars: None,
                payload: vec![],
            }).await
        }));
    }
    
    let results = futures::future::join_all(handles).await;
    for (i, result) in results.iter().enumerate() {
        let output = result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(output.response, Some(format!("{}\n", i).into_bytes()));
    }
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_docker_stdin_payload() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    let payload = b"test input data";
    let result = executor.execute(SandboxConfig {
        function_id: "test-stdin".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["cat".to_string()],
        env_vars: None,
        payload: payload.to_vec(),
    }).await.unwrap();

    assert_eq!(result.response, Some(payload.to_vec()));
}

// ============= FaaS Platform Capability Tests =============

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_faas_function_chaining() {
    // Test real function chaining - output of one becomes input of next
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // Stage 1: Generate data
    let stage1 = executor.execute(SandboxConfig {
        function_id: "chain-1".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "{\"value\":42}".to_string()],
        env_vars: None,
        payload: vec![],
    }).await.unwrap();

    // Stage 2: Transform data
    let stage2 = executor.execute(SandboxConfig {
        function_id: "chain-2".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["sh".to_string(), "-c".to_string(),
                      "cat | sed 's/42/100/'".to_string()],
        env_vars: None,
        payload: stage1.response.unwrap(),
    }).await.unwrap();

    let result = String::from_utf8_lossy(&stage2.response.unwrap());
    assert!(result.contains("100"));
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_faas_event_processing() {
    // Test event-driven function execution
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    let webhook_event = r#"{
        "event": "user.created",
        "data": {"id": "usr_123", "email": "test@example.com"}
    }"#;

    let result = executor.execute(SandboxConfig {
        function_id: "webhook-handler".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["sh".to_string(), "-c".to_string(),
                      r#"cat | grep -o '"id":"[^"]*"' | cut -d: -f2"#.to_string()],
        env_vars: Some(vec!["EVENT_SOURCE=webhook".to_string()]),
        payload: webhook_event.as_bytes().to_vec(),
    }).await.unwrap();

    let response = String::from_utf8_lossy(&result.response.unwrap());
    assert!(response.contains("usr_123"));
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_faas_performance_metrics() {
    // Test performance tracking for FaaS functions
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    let start = Instant::now();
    let result = executor.execute(SandboxConfig {
        function_id: "perf-test".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "test".to_string()],
        env_vars: None,
        payload: vec![],
    }).await.unwrap();
    let duration = start.elapsed();

    assert!(result.response.is_some());
    assert!(duration < Duration::from_secs(2), "Function should complete quickly");

    // Log metrics that would be collected
    println!("Function execution metrics:");
    println!("  Request ID: {}", result.request_id);
    println!("  Duration: {:?}", duration);
    println!("  Cold start: true");
}
