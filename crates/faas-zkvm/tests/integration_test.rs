//! Integration test: ZK Prover HTTP Server + Client
//!
//! Tests end-to-end ZK proof generation:
//! 1. Start faas-zk-prover HTTP server (must be running on localhost:8081)
//! 2. Use ZkProverClient to request proof
//! 3. Verify proof is returned correctly
//!
//! Run with: cargo test --package faas-zkvm --test integration_test -- --ignored
//! (Requires faas-zk-prover server running: cargo run --release --package faas-zk-prover)

use faas_zkvm::ZkProverClient;

#[tokio::test]
#[ignore] // Requires server to be running
async fn test_zk_prover_integration() {
    // Connect to locally running ZK prover service
    let client = ZkProverClient::new("http://localhost:8081");

    // Health check
    client
        .health()
        .await
        .expect("Server health check failed - is faas-zk-prover running?");

    println!("✓ Server health check passed");

    // Request ZK proof for Fibonacci(10)
    println!("Requesting ZK proof for Fibonacci(10)...");
    let proof = client
        .prove("fibonacci", vec!["10".to_string()], vec![])
        .await
        .expect("Proof generation failed");

    // Verify proof structure
    assert_eq!(proof.program, "fibonacci");
    assert_eq!(proof.public_inputs, vec!["10"]);
    assert!(!proof.proof_id.is_empty(), "Proof ID should not be empty");
    assert!(!proof.proof_data.is_empty(), "Proof data should not be empty");
    assert_eq!(proof.backend, "SP1 Local");
    assert_eq!(proof.execution_mode, "remote");
    assert!(proof.proving_time_ms > 0, "Proving time should be > 0");

    println!("✓ Proof generated successfully");
    println!("  Proof ID: {}...", &proof.proof_id[..16.min(proof.proof_id.len())]);
    println!("  Proof size: {} bytes", proof.proof_data.len());
    println!("  Proving time: {}ms", proof.proving_time_ms);
    println!("  Backend: {}", proof.backend);
}

#[tokio::test]
#[ignore]
async fn test_hash_preimage_proof() {
    let client = ZkProverClient::new("http://localhost:8081");

    println!("Requesting ZK proof for hash preimage...");
    let secret = "test_secret";
    let proof = client
        .prove("hash_preimage", vec![secret.to_string()], vec![])
        .await
        .expect("Hash preimage proof failed");

    assert_eq!(proof.program, "hash_preimage");
    assert!(!proof.proof_data.is_empty());

    println!("✓ Hash preimage proof generated");
    println!("  Proof size: {} bytes", proof.proof_data.len());
}
