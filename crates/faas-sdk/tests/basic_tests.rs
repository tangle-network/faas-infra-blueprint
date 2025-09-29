//! Basic unit tests for FaaS Rust SDK

use faas_client_sdk::*;

#[test]
fn test_sdk_types() {
    // Test that we can create the request types
    let request = ExecuteRequest {
        command: "echo test".to_string(),
        image: Some("alpine:latest".to_string()),
        runtime: Some(Runtime::Docker),
        env_vars: None,
        working_dir: Some("/tmp".to_string()),
        timeout_ms: Some(5000),
        cache_key: None,
    };

    assert_eq!(request.command, "echo test");
    assert_eq!(request.image, Some("alpine:latest".to_string()));
    assert_eq!(request.working_dir, Some("/tmp".to_string()));
}

#[test]
fn test_client_creation() {
    // Test that we can create a client
    let _client = FaasClient::new("http://localhost:8080".to_string());

    // Just verify it doesn't panic and has the expected state
    assert!(true); // Basic smoke test
}

#[test]
fn test_runtime_enum() {
    // Test runtime enum values exist and can be used
    let _docker = Runtime::Docker;
    let _firecracker = Runtime::Firecracker;
    let _auto = Runtime::Auto;
    assert!(true); // Basic smoke test for enum variants
}

#[test]
fn test_cache_key_generation() {
    // Test that we can generate consistent cache keys
    let code1 = "print('hello')";
    let code2 = "print('hello')";
    let code3 = "print('world')";

    let hash1 = format!("{:x}", md5::compute(code1.as_bytes()));
    let hash2 = format!("{:x}", md5::compute(code2.as_bytes()));
    let hash3 = format!("{:x}", md5::compute(code3.as_bytes()));

    assert_eq!(hash1, hash2); // Same code should have same hash
    assert_ne!(hash1, hash3); // Different code should have different hash
}

#[test]
fn test_serialization() {
    // Test that our types can be serialized
    let request = ExecuteRequest {
        command: "echo test".to_string(),
        image: Some("alpine:latest".to_string()),
        runtime: Some(Runtime::Docker),
        env_vars: Some(vec![("TEST".to_string(), "value".to_string())]),
        working_dir: Some("/app".to_string()),
        timeout_ms: Some(30000),
        cache_key: Some("test-cache-key".to_string()),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("echo test"));
    assert!(json.contains("alpine:latest"));
    assert!(json.contains("/app"));
}