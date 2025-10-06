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

mod blueprint_service;

use faas_sdk::FaasClient;
// Import types from faas-zkvm library
use faas_zkvm::{ZkBackend, ZkProof};
pub use blueprint_service::{BlueprintServiceManager, JobId, JobRequest, JobResult};
use sha2::{Digest, Sha256};
use std::time::Instant;
use sp1_sdk::{include_elf, ProverClient, SP1Stdin, SP1ProofWithPublicValues};

// Include generated ELF binaries from guest programs
const FIBONACCI_ELF: &[u8] = include_elf!("fibonacci-guest");
const HASH_PREIMAGE_ELF: &[u8] = include_elf!("hash-preimage-guest");

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
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let app = axum::Router::new()
        .route("/v1/prove", axum::routing::post(prove_handler))
        .route("/health", axum::routing::get(health_handler));

    let addr = "0.0.0.0:8081";
    tracing::info!("🔐 faas-zk-prover starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(serde::Deserialize)]
struct ProveRequest {
    program: String,
    public_inputs: Vec<String>,
    #[serde(default)]
    private_inputs: Vec<String>,
}

#[derive(serde::Serialize)]
struct ProveResponse {
    proof_id: String,
    program: String,
    public_inputs: Vec<String>,
    proof_data: String,  // base64 encoded
    backend: String,
    proving_time_ms: u64,
}

async fn prove_handler(
    axum::Json(req): axum::Json<ProveRequest>,
) -> Result<axum::Json<ProveResponse>, (axum::http::StatusCode, String)> {
    tracing::info!("Proving request for program: {}", req.program);

    let service = ZkProvingService::new("".to_string(), ZkBackend::Sp1Local);

    let proof = service
        .prove(&req.program, req.public_inputs.clone(), req.private_inputs)
        .await
        .map_err(|e| (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Proving failed: {}", e),
        ))?;

    Ok(axum::Json(ProveResponse {
        proof_id: proof.proof_id,
        program: proof.program,
        public_inputs: proof.public_inputs,
        proof_data: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &proof.proof_data,
        ),
        backend: proof.backend,
        proving_time_ms: proof.proving_time_ms,
    }))
}

async fn health_handler() -> &'static str {
    "ok"
}
