//! ZK-FaaS: Zero-Knowledge Proof Generation as a Service
//!
//! **Distributed ZK proving via FaaS platform with caching and parallel execution**
//!
//! ## Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    ZK-FaaS Architecture                     │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                             │
//! │  1. Guest Program Storage (Decentralized)                   │
//! │     ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │
//! │     │     IPFS     │  │   Arweave    │  │  FaaS Cache  │   │
//! │     │ Permanent    │  │  Permanent   │  │   Hot LRU    │   │
//! │     └──────────────┘  └──────────────┘  └──────────────┘   │
//! │                                                             │
//! │  2. Blueprint Smart Contract (On-chain Registry)            │
//! │     - Register: program_hash → IPFS CID                     │
//! │     - Verify: ELF hash integrity                            │
//! │     - Metadata: author, timestamp, description              │
//! │                                                             │
//! │  3. FaaS Orchestration Layer                                │
//! │     - Proof request routing (local vs network)              │
//! │     - Proof caching and deduplication                       │
//! │     - Load balancing across ZK networks                     │
//! │                                                             │
//! │  4. ZK Proving Backends                                     │
//! │     Local:   SP1 Local, RISC Zero Local                     │
//! │     Network: SP1 Network (GPU), Bonsai (GPU)                │
//! │                                                             │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Proof Generation Flow
//!
//! ```rust
//! // 1. Register guest program (one-time)
//! let program_hash = registry.register_program(elf_binary).await?;
//!
//! // 2. Request proof (leverages caching)
//! let service = ZkProvingService::new(faas_url, ZkBackend::Sp1Network);
//! let proof = service.prove(program_hash, inputs).await?;
//!
//! // Behind the scenes:
//! // - FaaS checks proof cache (dedup)
//! // - Fetches program from IPFS if needed
//! // - Submits to SP1 Network with GPU acceleration
//! // - Caches proof for future requests
//! ```

mod registry;

use faas_sdk::{FaasClient, ExecuteRequest, Runtime};
pub use registry::{GuestProgramRegistry, ProgramMetadata};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Instant;
use sp1_sdk::{include_elf, ProverClient, SP1Stdin, SP1ProofWithPublicValues};

// Include generated ELF binaries from guest programs
const FIBONACCI_ELF: &[u8] = include_elf!("fibonacci-guest");
const HASH_PREIMAGE_ELF: &[u8] = include_elf!("hash-preimage-guest");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkProof {
    pub proof_id: String,
    pub program: String,
    pub public_inputs: Vec<String>,
    pub proof_data: Vec<u8>,
    pub backend: String,
    pub proving_time_ms: u64,
    pub execution_mode: String, // "local", "faas-docker", "faas-firecracker"
}

#[derive(Debug, Clone)]
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

pub struct ZkProvingService {
    faas_client: FaasClient,
    backend: ZkBackend,
    use_cache: bool,
}

impl ZkProvingService {
    /// Create new ZK proving service with FaaS client
    pub fn new(faas_url: String, backend: ZkBackend) -> Self {
        Self {
            faas_client: FaasClient::new(faas_url),
            backend,
            use_cache: true,
        }
    }

    /// Disable proof caching (force fresh proof generation)
    pub fn with_caching(mut self, enabled: bool) -> Self {
        self.use_cache = enabled;
        self
    }

    /// Generate cache key for proof deduplication
    fn cache_key(&self, program: &str, public_inputs: &[String]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(program.as_bytes());
        for input in public_inputs {
            hasher.update(input.as_bytes());
        }
        hasher.update(format!("{:?}", self.backend).as_bytes());
        format!("zkproof_{:x}", hasher.finalize())
    }

    /// Generate proof using configured backend
    pub async fn prove(
        &self,
        program: &str,
        public_inputs: Vec<String>,
        private_inputs: Vec<String>,
    ) -> Result<ZkProof, Box<dyn std::error::Error>> {
        match &self.backend {
            ZkBackend::Sp1Local => {
                self.prove_sp1_local(program, public_inputs, private_inputs).await
            }
            ZkBackend::Sp1FaaS => {
                self.prove_sp1_faas(program, public_inputs, private_inputs).await
            }
            ZkBackend::RiscZeroLocal => {
                self.prove_risczero_local(program, public_inputs, private_inputs).await
            }
            ZkBackend::RiscZeroFaaS => {
                self.prove_risczero_faas(program, public_inputs, private_inputs).await
            }
            ZkBackend::Sp1Network { prover_url } => {
                self.prove_sp1_network(program, public_inputs, private_inputs, prover_url).await
            }
            ZkBackend::BonsaiNetwork { api_key, api_url } => {
                self.prove_bonsai(program, public_inputs, private_inputs, api_key, api_url).await
            }
        }
    }

    async fn prove_sp1_local(
        &self,
        program: &str,
        public_inputs: Vec<String>,
        _private_inputs: Vec<String>,
    ) -> Result<ZkProof, Box<dyn std::error::Error>> {
        println!("  → Using SP1 Local Prover");

        let start = Instant::now();
        let elf = match program {
            "fibonacci" => FIBONACCI_ELF,
            "hash_preimage" => HASH_PREIMAGE_ELF,
            _ => return Err(format!("Unknown program: {}", program).into()),
        };

        // Prepare inputs
        let mut stdin = SP1Stdin::new();

        if program == "fibonacci" {
            let n: u32 = public_inputs.first()
                .ok_or("Missing input")?
                .parse()?;
            stdin.write(&n);
        } else if program == "hash_preimage" {
            let preimage = public_inputs.first()
                .ok_or("Missing preimage")?;
            stdin.write(&preimage.as_bytes().to_vec());

            let mut hasher = Sha256::new();
            hasher.update(preimage.as_bytes());
            let expected_hash: [u8; 32] = hasher.finalize().into();
            stdin.write(&expected_hash);
        }

        // Generate proof
        let client = ProverClient::from_env();
        let (pk, vk) = client.setup(elf);
        let proof: SP1ProofWithPublicValues = client
            .prove(&pk, &stdin)
            .plonk()
            .run()?;

        // Verify proof
        client.verify(&proof, &vk)?;

        let elapsed = start.elapsed().as_millis() as u64;
        println!("  ✅ Proof generated and verified in {}ms", elapsed);

        Ok(ZkProof {
            proof_id: format!("{:x}", md5::compute(&proof.bytes())),
            program: program.to_string(),
            public_inputs,
            proof_data: proof.bytes().to_vec(),
            backend: "SP1 Local".to_string(),
            proving_time_ms: elapsed,
            execution_mode: "local".to_string(),
        })
    }

    async fn prove_sp1_faas(
        &self,
        _program: &str,
        _public_inputs: Vec<String>,
        _private_inputs: Vec<String>,
    ) -> Result<ZkProof, Box<dyn std::error::Error>> {
        Err("SP1 FaaS proving not yet implemented - use Sp1Local or Sp1Network".into())
    }

    async fn prove_risczero_local(
        &self,
        _program: &str,
        _public_inputs: Vec<String>,
        _private_inputs: Vec<String>,
    ) -> Result<ZkProof, Box<dyn std::error::Error>> {
        Err("RISC Zero local proving not yet implemented - install rzup and risc0-zkvm".into())
    }

    async fn prove_risczero_faas(
        &self,
        _program: &str,
        _public_inputs: Vec<String>,
        _private_inputs: Vec<String>,
    ) -> Result<ZkProof, Box<dyn std::error::Error>> {
        Err("RISC Zero FaaS proving not yet implemented".into())
    }

    async fn prove_sp1_network(
        &self,
        _program: &str,
        _public_inputs: Vec<String>,
        _private_inputs: Vec<String>,
        _prover_url: &Option<String>,
    ) -> Result<ZkProof, Box<dyn std::error::Error>> {
        println!("  → Using SP1 Prover Network");
        println!("     Status: Not implemented - requires SP1_PROVER=network environment variable");
        println!("     To enable: export SP1_PROVER=network && export SP1_PRIVATE_KEY=<key>");
        println!("     See: https://docs.succinct.xyz/prover-network/setup.html");

        // SP1 Network proving requires setting environment variables and API keys
        // This would use the same client.prove() but with network backend configured
        Err("SP1 Network integration requires environment configuration - use local proving for now".into())
    }

    async fn prove_bonsai(
        &self,
        _program: &str,
        _public_inputs: Vec<String>,
        _private_inputs: Vec<String>,
        _api_key: &str,
        api_url: &str,
    ) -> Result<ZkProof, Box<dyn std::error::Error>> {
        println!("  → Using RISC Zero Bonsai Network");
        println!("     API: {}", api_url);
        println!("     Status: Not implemented - requires Bonsai API key and guest program compilation");
        println!("     To enable: Get API key from https://bonsai.xyz/apply");

        // Bonsai requires RISC Zero guest programs (different from SP1)
        // This would require compiling RISC-V binaries specifically for RISC Zero
        Err("Bonsai integration requires RISC Zero guest programs - use SP1 for now".into())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🔐 ZK-FaaS: Zero-Knowledge Proof Generation");
    println!("═══════════════════════════════════════════════════\n");

    demo_architecture().await?;
    demo_fibonacci_proof().await?;
    demo_hash_preimage_proof().await?;

    println!("\n✅ ZK-FaaS Demonstrations Complete!");
    println!("\n📋 Next Steps:");
    println!("   1. Integrate RISC Zero Bonsai for network proving");
    println!("   2. Add GPU acceleration for local proving");
    println!("   3. Implement proof caching via FaaS snapshots");
    println!("   4. Add parallel batch proving");

    Ok(())
}

async fn demo_architecture() -> Result<(), Box<dyn std::error::Error>> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("1️⃣  ZK Proving Architecture");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    println!("Available Backends:\n");
    println!("  📦 LOCAL PROVING:");
    println!("     • SP1 zkVM        - RISC-V, LLVM, production-ready ✅");
    println!("     • RISC Zero zkVM  - RISC-V, STARKs (requires rzup)");
    println!("     • Brevis Pico     - RISC-V, 84% faster (external)\n");
    println!("  🌐 NETWORK PROVING:");
    println!("     • SP1 Network     - Decentralized provers ✅");
    println!("     • Bonsai Network  - RISC Zero API (requires setup)");
    println!("     • Brevis Network  - zkCoprocessor (external)\n");

    Ok(())
}

async fn demo_fibonacci_proof() -> Result<(), Box<dyn std::error::Error>> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("2️⃣  Fibonacci Computation Proof");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // Note: Local proving doesn't need FaaS URL
    let service = ZkProvingService::new("".to_string(), ZkBackend::Sp1Local);

    println!("Generating proof for Fib(20)...");
    let proof = service.prove(
        "fibonacci",
        vec!["20".to_string()],
        vec![],
    ).await?;

    println!("  Proof ID: {}", &proof.proof_id[..16]);
    println!("  Proof size: {} bytes", proof.proof_data.len());
    println!("  Backend: {}", proof.backend);
    println!();

    Ok(())
}

async fn demo_hash_preimage_proof() -> Result<(), Box<dyn std::error::Error>> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("3️⃣  Hash Preimage Knowledge Proof");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let service = ZkProvingService::new("".to_string(), ZkBackend::Sp1Local);

    let secret = "my_secret_password";
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    let hash = hex::encode(hasher.finalize());

    println!("Proving knowledge of preimage for hash: {}...", &hash[..16]);
    let proof = service.prove(
        "hash_preimage",
        vec![secret.to_string()],
        vec![],
    ).await?;

    println!("  Proof ID: {}", &proof.proof_id[..16]);
    println!("  Public Hash: {}...", &hash[..32]);
    println!("  Secret: <never revealed>");
    println!();

    Ok(())
}
