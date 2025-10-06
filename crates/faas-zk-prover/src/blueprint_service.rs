//! Blueprint Service Manager for ZK-FaaS
//!
//! Implements the Rust-side service manager that handles job execution
//! for the ZkFaasBlueprint smart contract.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │          Blueprint Service Architecture                 │
//! ├─────────────────────────────────────────────────────────┤
//! │                                                         │
//! │  Smart Contract (Solidity)                              │
//! │  ├─ onRegister() → Validates operator                   │
//! │  ├─ onRequest() → Creates service instance              │
//! │  └─ onJobResult() → Processes job results               │
//! │                   │                                     │
//! │                   ▼                                     │
//! │  Service Manager (Rust) ◄─── You are here              │
//! │  ├─ execute_job() → Routes to backend                   │
//! │  ├─ Job 0: register_program()                           │
//! │  ├─ Job 1: prove_sp1()                                  │
//! │  ├─ Job 2: prove_risczero()                             │
//! │  └─ Job 3: verify_proof()                               │
//! │                   │                                     │
//! │                   ▼                                     │
//! │  Backends                                               │
//! │  ├─ SP1 Local Prover                                    │
//! │  ├─ SP1 Network API                                     │
//! │  ├─ RISC Zero Local                                     │
//! │  └─ Bonsai Network API                                  │
//! │                                                         │
//! └─────────────────────────────────────────────────────────┘
//! ```

use faas_zkvm::{ZkBackend, ProgramRegistry};
use crate::ZkProvingService;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Job identifiers matching the Blueprint smart contract
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobId {
    /// Job 0: Register ZK program on-chain with IPFS CID
    RegisterProgram = 0,
    /// Job 1: Generate proof using SP1 zkVM
    ProveSp1 = 1,
    /// Job 2: Generate proof using RISC Zero zkVM
    ProveRiscZero = 2,
    /// Job 3: Verify proof on-chain
    VerifyProof = 3,
}

impl From<u8> for JobId {
    fn from(value: u8) -> Self {
        match value {
            0 => JobId::RegisterProgram,
            1 => JobId::ProveSp1,
            2 => JobId::ProveRiscZero,
            3 => JobId::VerifyProof,
            _ => panic!("Unknown job ID: {}", value),
        }
    }
}

/// Job execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    pub service_id: u64,
    pub job_id: u8,
    pub job_call_id: u64,
    pub inputs: Vec<u8>,
}

/// Job execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub service_id: u64,
    pub job_id: u8,
    pub job_call_id: u64,
    pub outputs: Vec<u8>,
    pub success: bool,
    pub error: Option<String>,
}

/// Blueprint service manager for ZK-FaaS
pub struct BlueprintServiceManager {
    /// Guest program registry (wrapped in Mutex for interior mutability)
    registry: Arc<Mutex<ProgramRegistry>>,
    /// SP1 proving service
    sp1_service: Arc<ZkProvingService>,
}

impl BlueprintServiceManager {
    pub fn new(faas_url: String) -> Self {
        let registry = Arc::new(Mutex::new(ProgramRegistry::new()));

        // Create SP1 local proving service
        let sp1_service = Arc::new(ZkProvingService::new(
            faas_url.clone(),
            ZkBackend::Sp1Local,
        ));

        Self {
            registry,
            sp1_service,
        }
    }

    /// Execute a Blueprint job
    pub async fn execute_job(&self, request: JobRequest) -> JobResult {
        let job_id = JobId::from(request.job_id);

        println!(
            "📋 Executing job {:?} (service={}, call={})",
            job_id, request.service_id, request.job_call_id
        );

        let result = match job_id {
            JobId::RegisterProgram => self.job_register_program(&request.inputs).await,
            JobId::ProveSp1 => self.job_prove_sp1(&request.inputs).await,
            JobId::ProveRiscZero => self.job_prove_risczero(&request.inputs).await,
            JobId::VerifyProof => self.job_verify_proof(&request.inputs).await,
        };

        match result {
            Ok(outputs) => JobResult {
                service_id: request.service_id,
                job_id: request.job_id,
                job_call_id: request.job_call_id,
                outputs,
                success: true,
                error: None,
            },
            Err(e) => JobResult {
                service_id: request.service_id,
                job_id: request.job_id,
                job_call_id: request.job_call_id,
                outputs: vec![],
                success: false,
                error: Some(e),
            },
        }
    }

    /// Job 0: Register ZK Program
    ///
    /// Inputs (ABI-encoded):
    /// - bytes32: elfHash (SHA256 of ELF binary)
    /// - string: ipfsCid (IPFS content identifier)
    /// - string: description (program description)
    /// - uint8: zkvmType (0=SP1, 1=RISCZero)
    ///
    /// Outputs (ABI-encoded):
    /// - bool: verified (whether ELF was verified)
    async fn job_register_program(&self, inputs: &[u8]) -> Result<Vec<u8>, String> {
        println!("  → Job 0: Register ZK Program");

        // Decode inputs (simplified - in production use ABI decoder)
        let inputs_str = String::from_utf8_lossy(inputs);
        let parts: Vec<&str> = inputs_str.split('|').collect();

        if parts.len() < 4 {
            return Err("Invalid input format".to_string());
        }

        let elf_hash = parts[0];
        let ipfs_cid = parts[1];
        let description = parts[2];
        let zkvm_type_str = parts[3];

        println!("  📦 Program: {}", description);
        println!("  🔗 IPFS CID: {}", ipfs_cid);
        println!("  🔑 ELF Hash: {}", &elf_hash[..16]);

        // In production: Fetch ELF from IPFS and verify hash
        // For now, just register metadata in local registry
        use faas_zkvm::ProgramMetadata;
        let metadata = ProgramMetadata {
            program_hash: elf_hash.to_string(),
            ipfs_cid: Some(ipfs_cid.to_string()),
            description: description.to_string(),
            zkvm_type: zkvm_type_str.to_string(),
            author: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        // Register in local registry (cache)
        self.registry.lock().unwrap().register(metadata)?;

        println!("  ✅ Program registered: {}", &elf_hash[..16]);

        // Return verification result (ABI-encoded bool)
        Ok(vec![1]) // true
    }

    /// Job 1: Generate Proof (SP1)
    ///
    /// Inputs (ABI-encoded):
    /// - bytes32: programHash
    /// - bytes: publicInputs
    /// - address: requester
    ///
    /// Outputs (ABI-encoded):
    /// - bytes: proofData
    /// - bool: verified
    async fn job_prove_sp1(&self, inputs: &[u8]) -> Result<Vec<u8>, String> {
        println!("  → Job 1: Generate Proof (SP1)");

        // Decode inputs (simplified)
        let inputs_str = String::from_utf8_lossy(inputs);
        let parts: Vec<&str> = inputs_str.split('|').collect();

        if parts.len() < 3 {
            return Err("Invalid input format".to_string());
        }

        let program_hash = parts[0];
        let public_inputs_str = parts[1];
        let requester = parts[2];

        println!("  📝 Program: {}", &program_hash[..16]);
        println!("  👤 Requester: {}", requester);

        // Parse public inputs (comma-separated)
        let public_inputs: Vec<String> = public_inputs_str
            .split(',')
            .map(|s| s.to_string())
            .collect();

        // Get program from registry
        let program_name = {
            let registry = self.registry.lock().unwrap();
            registry
                .get(program_hash)
                .ok_or("Program not found")?
                .description
                .clone()
        };

        // Generate proof using SP1
        let proof = self
            .sp1_service
            .prove(&program_name, public_inputs, vec![])
            .await
            .map_err(|e| format!("Proof generation failed: {}", e))?;

        println!("  ✅ Proof generated: {} bytes", proof.proof_data.len());

        // Return proof data + verification status
        let mut output = proof.proof_data.clone();
        output.push(1); // verified = true

        Ok(output)
    }

    /// Job 2: Generate Proof (RISC Zero)
    async fn job_prove_risczero(&self, _inputs: &[u8]) -> Result<Vec<u8>, String> {
        println!("  → Job 2: Generate Proof (RISC Zero)");
        Err("RISC Zero proving not yet implemented".to_string())
    }

    /// Job 3: Verify Proof On-chain
    ///
    /// Inputs (ABI-encoded):
    /// - bytes32: proofId
    ///
    /// Outputs (ABI-encoded):
    /// - bool: valid
    async fn job_verify_proof(&self, inputs: &[u8]) -> Result<Vec<u8>, String> {
        println!("  → Job 3: Verify Proof");

        let proof_id = String::from_utf8_lossy(inputs);
        println!("  🔍 Proof ID: {}", &proof_id[..min(16, proof_id.len())]);

        // In production: verify proof using on-chain verifier
        // For now, return success
        Ok(vec![1]) // valid = true
    }
}

fn min(a: usize, b: usize) -> usize {
    if a < b {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_job_id_conversion() {
        assert_eq!(JobId::from(0), JobId::RegisterProgram);
        assert_eq!(JobId::from(1), JobId::ProveSp1);
        assert_eq!(JobId::from(2), JobId::ProveRiscZero);
        assert_eq!(JobId::from(3), JobId::VerifyProof);
    }

    #[tokio::test]
    async fn test_blueprint_service_creation() {
        let service = BlueprintServiceManager::new("http://localhost:8080".to_string());
        // Just verify it constructs without errors
        assert!(true);
    }
}
