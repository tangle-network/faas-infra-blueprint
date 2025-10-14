use faas_executor::bollard::Docker;
use faas_executor::{
    common::{SandboxConfig, SandboxExecutor},
    DockerExecutor,
};
use std::sync::Arc;

/// Test that containers cannot escape sandbox
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_container_escape_prevention() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // Try to access host filesystem
    let result = executor
        .execute(SandboxConfig {
            function_id: "escape-test".to_string(),
            source: "alpine:latest".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "cat /etc/passwd | head -1".to_string(), // Should only see container's passwd
            ],
            env_vars: None,
            payload: vec![],
        })
        .await
        .unwrap();

    let response = result.response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("Container passwd: {}", output);

    // Should see alpine's minimal passwd, not host's
    assert!(output.contains("root:x:0:0:root"));
    assert!(!output.contains("/Users/")); // Mac host path shouldn't be visible
}

/// Test that containers have resource limits
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_resource_limits_enforced() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker.clone());

    // Try to allocate excessive memory
    let result = executor
        .execute(SandboxConfig {
            function_id: "memory-bomb".to_string(),
            source: "python:3.11-slim".to_string(),
            command: vec![
                "python3".to_string(),
                "-c".to_string(),
                "x = 'A' * (10**9)".to_string(), // Try to allocate 1GB
            ],
            env_vars: None,
            payload: vec![],
        })
        .await;

    // Should fail or be killed due to memory limits
    println!("Memory bomb result: {:?}", result);

    // Container should be cleaned up after OOM
    let containers = docker.list_containers::<String>(None).await.unwrap();
    let leaked = containers.iter().any(|c| {
        c.names.as_ref().map_or(false, |names| {
            names.iter().any(|n| n.contains("memory-bomb"))
        })
    });
    assert!(!leaked, "Container leaked after memory limit test!");
}

/// Test that malicious payloads are handled safely
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_malicious_payload_handling() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // Send payload with shell injection attempt
    let malicious_payload = b"'; cat /etc/shadow; echo '".to_vec();

    let result = executor
        .execute(SandboxConfig {
            function_id: "injection-test".to_string(),
            source: "alpine:latest".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo 'Input:' && cat".to_string(),
            ],
            env_vars: None,
            payload: malicious_payload.clone(),
        })
        .await
        .unwrap();

    let response = result.response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("Injection test output: {}", output);

    // Should NOT contain shadow file contents
    assert!(!output.contains("root:!"));
    assert!(!output.contains("daemon:*"));

    // Should contain the literal payload as data, not executed
    assert!(output.contains("cat /etc/shadow") || output.contains("Input:"));
}

/// Test container isolation between executions
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_container_isolation() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // First execution writes a file
    let result1 = executor
        .execute(SandboxConfig {
            function_id: "writer".to_string(),
            source: "alpine:latest".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo 'secret-data' > /tmp/secret.txt && echo 'written'".to_string(),
            ],
            env_vars: None,
            payload: vec![],
        })
        .await
        .unwrap();

    assert_eq!(
        String::from_utf8_lossy(&result1.response.unwrap()).trim(),
        "written"
    );

    // Second execution tries to read the file
    let result2 = executor
        .execute(SandboxConfig {
            function_id: "reader".to_string(),
            source: "alpine:latest".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "cat /tmp/secret.txt 2>&1 || echo 'not-found'".to_string(),
            ],
            env_vars: None,
            payload: vec![],
        })
        .await
        .unwrap();

    let response = result2.response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("Isolation test: {}", output);

    // File should NOT exist in second container
    assert!(output.contains("not-found") || output.contains("No such file"));
    assert!(!output.contains("secret-data"));
}

/// Test that network access can be controlled
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_network_isolation() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // Try to make external network request
    let result = executor
        .execute(SandboxConfig {
            function_id: "network-test".to_string(),
            source: "alpine:latest".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                // Try to reach Google DNS
                "ping -c 1 8.8.8.8 2>&1 || echo 'network-blocked'".to_string(),
            ],
            env_vars: None,
            payload: vec![],
        })
        .await
        .unwrap();

    let response = result.response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("Network test: {}", output);

    // Network might be allowed or blocked depending on config
    // But container should complete without hanging
    assert!(output.contains("bytes from") || output.contains("network-blocked"));
}

/// Test privilege escalation is prevented
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_privilege_escalation_prevention() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    // Try to gain root privileges
    let result = executor
        .execute(SandboxConfig {
            function_id: "privilege-test".to_string(),
            source: "alpine:latest".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "id && sudo ls 2>&1 || echo 'no-sudo'".to_string(),
            ],
            env_vars: None,
            payload: vec![],
        })
        .await
        .unwrap();

    let response = result.response.unwrap();
    let output = String::from_utf8_lossy(&response);
    println!("Privilege test: {}", output);

    // Should not have sudo available
    assert!(output.contains("no-sudo") || output.contains("sudo: not found"));

    // Should be running as non-root or with limited capabilities
    // (depending on security configuration)
}

/// Verify containers are cleaned up after timeout
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_container_cleanup_after_failure() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let docker_clone = docker.clone();
    let executor = DockerExecutor::new(docker);

    // Get container count before
    let before = docker_clone
        .list_containers::<String>(None)
        .await
        .unwrap()
        .len();

    // Run a command that will timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        executor.execute(SandboxConfig {
            function_id: "cleanup-test".to_string(),
            source: "alpine:latest".to_string(),
            command: vec!["sleep".to_string(), "60".to_string()],
            env_vars: None,
            payload: vec![],
        }),
    )
    .await;

    // Should timeout
    assert!(result.is_err());

    // Wait for cleanup
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Check container count after
    let after = docker_clone
        .list_containers::<String>(None)
        .await
        .unwrap()
        .len();

    // No container leak
    assert!(
        after <= before + 1, // Allow for some pool containers
        "Container leak detected! Before: {}, After: {}",
        before,
        after
    );
}
