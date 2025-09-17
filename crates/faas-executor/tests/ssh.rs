use chrono::{Duration as ChronoDuration, Utc};
use faas_executor::ssh::{
    SshConfig, SshConnectionManager, SshKeyAlgorithm, SshKeyManager,
};
use std::fs;
use tempfile::tempdir;

/// Test basic SSH key generation
#[tokio::test]
async fn test_basic_key_generation() {
    let temp_dir = tempdir().unwrap();
    let config = SshConfig::default();

    let manager = SshKeyManager::new(temp_dir.path(), config);
    let key = manager.generate_keypair().await.unwrap();

    assert!(!key.private_key.is_empty());
    assert!(!key.public_key.is_empty());
    assert!(!key.fingerprint.is_empty());
    assert!(!key.algorithm.is_empty());
    assert!(!key.id.is_empty());
    assert!(key.created_at <= Utc::now());
}

/// Test SSH connection manager basic functionality
#[tokio::test]
async fn test_connection_manager_basic() {
    let temp_dir = tempdir().unwrap();
    let config = SshConfig::default();
    let key_manager = SshKeyManager::new(temp_dir.path(), config);

    let mut conn_manager = SshConnectionManager::new(key_manager);

    // Connect to instance (simulated)
    let key1 = conn_manager
        .connect("instance1", "localhost", 22)
        .await
        .unwrap();
    assert!(!key1.private_key.is_empty());
    assert!(!key1.public_key.is_empty());

    // Reconnect should reuse existing key
    let key2 = conn_manager
        .connect("instance1", "localhost", 22)
        .await
        .unwrap();
    assert_eq!(key1.id, key2.id);

    // Disconnect
    conn_manager.disconnect("instance1").await.unwrap();
}

/// Test key rotation functionality
#[tokio::test]
async fn test_key_rotation_basic() {
    let temp_dir = tempdir().unwrap();
    let config = SshConfig {
        auto_rotate: true,
        rotation_interval: ChronoDuration::days(30),
        key_algorithm: SshKeyAlgorithm::Ed25519,
        max_keys_per_instance: 3,
    };

    let mut manager = SshKeyManager::new(temp_dir.path(), config);

    // Generate initial key
    let initial_key = manager.generate_keypair().await.unwrap();
    assert!(initial_key.rotated_from.is_none());

    // Rotate key
    let rotated_key = manager
        .rotate_key("instance1", &initial_key.id)
        .await
        .unwrap();

    assert_ne!(rotated_key.id, initial_key.id);
    assert_eq!(rotated_key.rotated_from, Some(initial_key.id.clone()));
    assert_ne!(rotated_key.fingerprint, initial_key.fingerprint);
    assert_eq!(rotated_key.algorithm, initial_key.algorithm);
}

/// Test key algorithm configuration
#[tokio::test]
async fn test_key_algorithms() {
    let temp_dir = tempdir().unwrap();

    // Test Ed25519
    let config_ed25519 = SshConfig {
        auto_rotate: false,
        rotation_interval: ChronoDuration::days(30),
        key_algorithm: SshKeyAlgorithm::Ed25519,
        max_keys_per_instance: 5,
    };

    let manager = SshKeyManager::new(temp_dir.path(), config_ed25519);
    let key_ed25519 = manager.generate_keypair().await.unwrap();

    assert_eq!(key_ed25519.algorithm, "Ed25519");
    assert!(key_ed25519.public_key.starts_with("ssh-ed25519"));
    assert!(key_ed25519.private_key.contains("BEGIN OPENSSH PRIVATE KEY"));
    assert!(key_ed25519.fingerprint.starts_with("SHA256:"));
}

/// Test key expiration functionality
#[tokio::test]
async fn test_key_expiration() {
    let temp_dir = tempdir().unwrap();
    let config = SshConfig {
        auto_rotate: true,
        rotation_interval: ChronoDuration::days(30),
        key_algorithm: SshKeyAlgorithm::Ed25519,
        max_keys_per_instance: 5,
    };

    let manager = SshKeyManager::new(temp_dir.path(), config);

    // Generate key
    let key = manager.generate_keypair().await.unwrap();

    // Fresh key should have expiration set
    assert!(key.expires_at.is_some());

    // Check that expiration is in the future
    if let Some(expires_at) = key.expires_at {
        assert!(expires_at > Utc::now());
    }
}

/// Test key revocation
#[tokio::test]
async fn test_key_revocation() {
    let temp_dir = tempdir().unwrap();
    let config = SshConfig::default();

    let manager = SshKeyManager::new(temp_dir.path(), config);

    // Generate key
    let key = manager.generate_keypair().await.unwrap();

    // Revoke key (this should succeed even if the key doesn't exist in a registry)
    let result = manager.revoke_key(&key.id).await;
    assert!(result.is_ok());
}

/// Test concurrent key operations
#[tokio::test]
async fn test_concurrent_key_generation() {
    use std::sync::Arc;

    let temp_dir = tempdir().unwrap();
    let config = SshConfig::default();

    let manager = Arc::new(SshKeyManager::new(temp_dir.path(), config));

    // Generate keys concurrently
    let handles: Vec<_> = (0..5)
        .map(|_| {
            let manager = Arc::clone(&manager);
            tokio::spawn(async move {
                manager.generate_keypair().await.unwrap()
            })
        })
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        let key = handle.await.unwrap();
        results.push(key);
    }

    // Verify all keys are unique
    let mut key_ids = std::collections::HashSet::new();
    for key in &results {
        assert!(key_ids.insert(key.id.clone()));
    }

    assert_eq!(results.len(), 5);
}