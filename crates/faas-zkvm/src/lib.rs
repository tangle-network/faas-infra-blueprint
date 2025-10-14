//! Zero-Knowledge Virtual Machine Integration for FaaS
//!
//! This library provides reusable infrastructure for ZK proof generation
//! across different zkVM backends (SP1, RISC Zero, etc.) using the FaaS platform.
//!
//! ## Architecture
//!
//! - **ZkBackend**: Enum for different proving backends (local, network, FaaS)
//! - **ZkProof**: Standard proof format across all backends
//! - **ProgramRegistry**: Program storage and caching (future: IPFS integration)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use faas_zkvm::{ZkBackend, ZkProof};
//!
//! // Define backend
//! let backend = ZkBackend::Sp1Local;
//!
//! // Generate proof using your service
//! // Implementation varies by backend
//! ```

use serde::{Deserialize, Serialize};

/// Simple HTTP client for ZK Prover service
pub struct ZkProverClient {
    base_url: String,
    http_client: reqwest::Client,
}

#[derive(thiserror::Error, Debug)]
pub enum ZkProverError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Prover service error: {0}")]
    Server(String),
}

impl ZkProverClient {
    /// Create new client pointing to ZK prover service
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http_client: reqwest::Client::new(),
        }
    }

    /// Request a ZK proof generation
    pub async fn prove(
        &self,
        program: &str,
        public_inputs: Vec<String>,
        private_inputs: Vec<String>,
    ) -> Result<ZkProof, ZkProverError> {
        #[derive(Serialize)]
        struct ProveRequest {
            program: String,
            public_inputs: Vec<String>,
            private_inputs: Vec<String>,
        }

        #[derive(Deserialize)]
        struct ProveResponse {
            proof_id: String,
            program: String,
            public_inputs: Vec<String>,
            proof_data: String, // base64
            backend: String,
            proving_time_ms: u64,
        }

        let req = ProveRequest {
            program: program.to_string(),
            public_inputs,
            private_inputs,
        };

        let resp = self
            .http_client
            .post(format!("{}/v1/prove", self.base_url))
            .json(&req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let error_msg = resp
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ZkProverError::Server(error_msg));
        }

        let prove_resp: ProveResponse = resp.json().await?;

        // Decode base64 proof data
        let proof_data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &prove_resp.proof_data,
        )
        .map_err(|e| ZkProverError::Server(format!("Invalid base64: {e}")))?;

        Ok(ZkProof {
            proof_id: prove_resp.proof_id,
            program: prove_resp.program,
            public_inputs: prove_resp.public_inputs,
            proof_data,
            backend: prove_resp.backend,
            proving_time_ms: prove_resp.proving_time_ms,
            execution_mode: "remote".to_string(),
        })
    }

    /// Health check
    pub async fn health(&self) -> Result<(), ZkProverError> {
        let resp = self
            .http_client
            .get(format!("{}/health", self.base_url))
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(ZkProverError::Server("Health check failed".to_string()))
        }
    }
}

/// Zero-knowledge proving backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ZkBackend {
    /// Local proving via SP1 (direct execution, no FaaS)
    Sp1Local,
    /// FaaS-distributed SP1 proving (Docker/Firecracker)
    Sp1FaaS,
    /// RISC Zero local proving
    RiscZeroLocal,
    /// FaaS-distributed RISC Zero proving
    RiscZeroFaaS,
    /// SP1 Network proving via Succinct prover network
    Sp1Network { prover_url: Option<String> },
    /// RISC Zero Bonsai Network
    BonsaiNetwork { api_key: String, api_url: String },
}

/// Zero-knowledge proof with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkProof {
    /// Unique proof identifier (hash of proof data)
    pub proof_id: String,
    /// Program identifier (name or hash)
    pub program: String,
    /// Public inputs to the proof
    pub public_inputs: Vec<String>,
    /// Serialized proof data (backend-specific format)
    pub proof_data: Vec<u8>,
    /// Backend that generated this proof
    pub backend: String,
    /// Time taken to generate proof (milliseconds)
    pub proving_time_ms: u64,
    /// Execution mode: "local", "faas-docker", "faas-firecracker"
    pub execution_mode: String,
}

/// Program metadata for registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramMetadata {
    /// Program hash (SHA256 of ELF binary)
    pub program_hash: String,
    /// IPFS CID (optional, for decentralized storage)
    pub ipfs_cid: Option<String>,
    /// Human-readable description
    pub description: String,
    /// zkVM type (sp1, risczero, etc.)
    pub zkvm_type: String,
    /// Author address
    pub author: Option<String>,
    /// Registration timestamp
    pub timestamp: u64,
}

/// Simple in-memory program registry (placeholder for future IPFS integration)
#[derive(Debug, Default)]
pub struct ProgramRegistry {
    programs: std::collections::HashMap<String, ProgramMetadata>,
}

impl ProgramRegistry {
    /// Create new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a program with metadata
    pub fn register(&mut self, metadata: ProgramMetadata) -> Result<(), String> {
        self.programs
            .insert(metadata.program_hash.clone(), metadata);
        Ok(())
    }

    /// Get program metadata by hash
    pub fn get(&self, program_hash: &str) -> Option<&ProgramMetadata> {
        self.programs.get(program_hash)
    }

    /// List all registered programs
    pub fn list(&self) -> Vec<&ProgramMetadata> {
        self.programs.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zkbackend_serialization() {
        let backend = ZkBackend::Sp1Local;
        let json = serde_json::to_string(&backend).unwrap();
        let deserialized: ZkBackend = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, ZkBackend::Sp1Local));
    }

    #[test]
    fn test_program_registry() {
        let mut registry = ProgramRegistry::new();

        let metadata = ProgramMetadata {
            program_hash: "abc123".to_string(),
            ipfs_cid: None,
            description: "Test program".to_string(),
            zkvm_type: "sp1".to_string(),
            author: None,
            timestamp: 0,
        };

        registry.register(metadata.clone()).unwrap();
        let retrieved = registry.get("abc123").unwrap();
        assert_eq!(retrieved.description, "Test program");
    }
}
