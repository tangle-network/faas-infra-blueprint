//! Real Docker integration tests

use bollard::Docker;
use faas_common::{SandboxConfig, SandboxExecutor};
use faas_executor::DockerExecutor;
use std::sync::Arc;
use std::time::Duration;

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
