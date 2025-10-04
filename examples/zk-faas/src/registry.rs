//! Guest Program Registry and Storage
//!
//! Decentralized storage for ZK guest programs with on-chain verification.
//!
//! Architecture:
//! - Guest programs stored on IPFS/Arweave (decentralized)
//! - Program metadata registered on Blueprint smart contract (on-chain)
//! - FaaS platform caches frequently-used programs (performance)
//!
//! ## Storage Layers
//!
//! 1. **Decentralized Storage (IPFS/Arweave)**
//!    - Permanent, content-addressed storage
//!    - Guest ELF binaries (~1-10 MB each)
//!    - Accessible via CID (Content Identifier)
//!
//! 2. **Blueprint Smart Contract (On-chain)**
//!    ```solidity
//!    mapping(bytes32 programHash => ProgramMetadata) public programs;
//!
//!    struct ProgramMetadata {
//!        string ipfsCid;        // IPFS content identifier
//!        bytes32 elfHash;       // SHA256 of ELF binary
//!        address author;        // Program creator
//!        uint256 timestamp;     // Registration time
//!        string description;    // Program purpose
//!    }
//!    ```
//!
//! 3. **FaaS Cache Layer (Performance)**
//!    - In-memory LRU cache for hot programs
//!    - Persistent disk cache for warm programs
//!    - Automatic cache invalidation on updates

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Program metadata stored on-chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramMetadata {
    /// IPFS CID for the ELF binary
    pub ipfs_cid: String,
    /// SHA256 hash of ELF binary for verification
    pub elf_hash: String,
    /// Program author address
    pub author: String,
    /// Registration timestamp
    pub timestamp: u64,
    /// Human-readable description
    pub description: String,
    /// zkVM type (SP1, RISC Zero, etc.)
    pub zkvm_type: String,
}

/// Guest program registry managing program storage and retrieval
pub struct GuestProgramRegistry {
    /// In-memory cache of program ELF binaries
    cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    /// Program metadata (would come from smart contract)
    metadata: Arc<RwLock<HashMap<String, ProgramMetadata>>>,
}

impl GuestProgramRegistry {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new guest program
    ///
    /// In production, this would:
    /// 1. Upload ELF to IPFS → get CID
    /// 2. Submit transaction to Blueprint contract
    /// 3. Cache locally
    pub async fn register_program(
        &self,
        elf_binary: Vec<u8>,
        description: String,
        zkvm_type: String,
    ) -> Result<String, String> {
        // Compute program hash
        let program_hash = self.compute_program_hash(&elf_binary);

        // Simulate IPFS upload (in production: upload_to_ipfs())
        let ipfs_cid = format!("Qm{}", &program_hash[..46]); // Mock CID

        // Create metadata
        let metadata = ProgramMetadata {
            ipfs_cid: ipfs_cid.clone(),
            elf_hash: program_hash.clone(),
            author: "0x0000000000000000000000000000000000000000".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            description,
            zkvm_type,
        };

        // Store metadata (in production: call smart contract)
        self.metadata.write().await.insert(program_hash.clone(), metadata);

        // Cache ELF binary
        self.cache.write().await.insert(program_hash.clone(), elf_binary);

        Ok(program_hash)
    }

    /// Retrieve guest program ELF binary
    ///
    /// Resolution order:
    /// 1. Check local cache (fast)
    /// 2. Fetch from IPFS using on-chain CID (slower)
    /// 3. Error if not found
    pub async fn get_program(&self, program_hash: &str) -> Result<Vec<u8>, String> {
        // Check cache first
        if let Some(elf) = self.cache.read().await.get(program_hash) {
            return Ok(elf.clone());
        }

        // Get metadata from registry (smart contract lookup)
        let metadata = self
            .metadata
            .read()
            .await
            .get(program_hash)
            .ok_or_else(|| format!("Program not found: {}", program_hash))?
            .clone();

        // Fetch from IPFS (in production: fetch_from_ipfs(&metadata.ipfs_cid))
        // For now, return error as we don't have IPFS integration yet
        Err(format!(
            "Program not in cache, would fetch from IPFS: {}",
            metadata.ipfs_cid
        ))
    }

    /// Get program metadata
    pub async fn get_metadata(&self, program_hash: &str) -> Option<ProgramMetadata> {
        self.metadata.read().await.get(program_hash).cloned()
    }

    /// Compute deterministic program hash
    fn compute_program_hash(&self, elf_binary: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(elf_binary);
        format!("{:x}", hasher.finalize())
    }

    /// List all registered programs
    pub async fn list_programs(&self) -> Vec<(String, ProgramMetadata)> {
        self.metadata
            .read()
            .await
            .iter()
            .map(|(hash, meta)| (hash.clone(), meta.clone()))
            .collect()
    }
}

impl Default for GuestProgramRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock IPFS client for development
///
/// In production, replace with:
/// - `ipfs-api-backend-actix` crate
/// - Pinata API
/// - Web3.Storage API
pub struct IpfsClient {
    // Mock implementation
}

impl IpfsClient {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }

    #[allow(dead_code)]
    pub async fn upload(&self, _data: &[u8]) -> Result<String, String> {
        // Would implement actual IPFS upload
        Err("IPFS upload not implemented - use Pinata or Web3.Storage".to_string())
    }

    #[allow(dead_code)]
    pub async fn fetch(&self, _cid: &str) -> Result<Vec<u8>, String> {
        // Would implement actual IPFS fetch
        Err("IPFS fetch not implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_program_registration() {
        let registry = GuestProgramRegistry::new();
        let elf = vec![0x7f, 0x45, 0x4c, 0x46]; // ELF magic bytes

        let hash = registry
            .register_program(elf.clone(), "Test program".to_string(), "SP1".to_string())
            .await
            .unwrap();

        assert!(!hash.is_empty());

        let retrieved = registry.get_program(&hash).await.unwrap();
        assert_eq!(retrieved, elf);
    }

    #[tokio::test]
    async fn test_program_metadata() {
        let registry = GuestProgramRegistry::new();
        let elf = vec![1, 2, 3, 4];

        let hash = registry
            .register_program(elf, "Metadata test".to_string(), "RISC Zero".to_string())
            .await
            .unwrap();

        let meta = registry.get_metadata(&hash).await.unwrap();
        assert_eq!(meta.description, "Metadata test");
        assert_eq!(meta.zkvm_type, "RISC Zero");
    }
}
