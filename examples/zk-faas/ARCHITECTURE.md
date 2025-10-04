# ZK-FaaS Architecture

**Zero-Knowledge Proof Generation as a Service via FaaS Platform**

## Executive Summary

ZK-FaaS provides a decentralized, scalable infrastructure for generating zero-knowledge proofs by combining:

1. **Decentralized Storage** (IPFS/Arweave) for guest programs
2. **Blueprint Smart Contracts** for on-chain program registry
3. **FaaS Platform** for proof orchestration and caching
4. **Network Proving APIs** (SP1 Network, Bonsai) for GPU-accelerated proving

This architecture eliminates single points of failure while leveraging existing ZK infrastructure.

## System Architecture

```
┌───────────────────────────────────────────────────────────────┐
│                    ZK-FaaS Full Stack                         │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│  Layer 1: Guest Program Storage (Decentralized)               │
│  ┌────────────┐  ┌────────────┐  ┌────────────────────────┐  │
│  │    IPFS    │  │  Arweave   │  │   FaaS LRU Cache       │  │
│  │ Permanent  │  │ Permanent  │  │   (Hot Programs)       │  │
│  │ Storage    │  │ Storage    │  │   Memory + Disk        │  │
│  └────────────┘  └────────────┘  └────────────────────────┘  │
│        │                │                      │               │
│        └────────────────┴──────────────────────┘               │
│                          │                                    │
│  Layer 2: On-chain Registry (Blueprint Smart Contract)        │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │  mapping(bytes32 => ProgramMetadata)                     │ │
│  │                                                          │ │
│  │  struct ProgramMetadata {                                │ │
│  │      string ipfsCid;        // IPFS content ID           │ │
│  │      bytes32 elfHash;       // SHA256(ELF binary)        │ │
│  │      address author;        // Program creator           │ │
│  │      uint256 timestamp;     // Registration time         │ │
│  │      string zkvm;           // "SP1" or "RISC Zero"      │ │
│  │  }                                                       │ │
│  └──────────────────────────────────────────────────────────┘ │
│                          │                                    │
│  Layer 3: FaaS Orchestration (Proof Request Handling)         │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │  ┌─────────────┐    ┌─────────────┐    ┌──────────────┐ │ │
│  │  │Proof Cache  │    │  Program     │    │  Request     │ │ │
│  │  │Deduplication│───▶│  Resolution  │───▶│  Routing     │ │ │
│  │  └─────────────┘    └─────────────┘    └──────────────┘ │ │
│  └──────────────────────────────────────────────────────────┘ │
│                          │                                    │
│  Layer 4: ZK Proving Backends (GPU-Accelerated)               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐   │
│  │ SP1 Network  │  │   Bonsai     │  │  Local Proving   │   │
│  │ (GPU Provers)│  │ (RISC Zero)  │  │  (Development)   │   │
│  └──────────────┘  └──────────────┘  └──────────────────┘   │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

## Proof Generation Workflow

### 1. Program Registration (One-Time)

```rust
// Developer creates and compiles guest program
let elf_binary = compile_guest_program("fibonacci");

// Upload to IPFS
let ipfs_cid = ipfs_client.upload(&elf_binary).await?;

// Register on Blueprint smart contract
let tx = blueprint_contract.register_program(
    program_hash,  // SHA256(elf_binary)
    ipfs_cid,
    "Fibonacci computation",
    "SP1"
).send().await?;
```

**Result:** Program is now discoverable on-chain and retrievable from IPFS.

### 2. Proof Request (Cached & Distributed)

```rust
// User requests proof
let service = ZkProvingService::new(
    "http://faas-gateway:8080".into(),
    ZkBackend::Sp1Network
);

let proof = service.prove(
    program_hash,
    vec!["1000".to_string()],  // public inputs
    vec![]                      // private inputs
).await?;
```

**Behind the Scenes:**

```
1. FaaS checks proof cache
   ├─ Hit? → Return cached proof ✓
   └─ Miss? → Continue to step 2

2. FaaS resolves program_hash
   ├─ Check FaaS program cache
   ├─ If miss: Query Blueprint contract for IPFS CID
   └─ Fetch ELF from IPFS

3. FaaS routes to proving backend
   ├─ SP1 Network → Submit ELF + inputs via API
   ├─ Bonsai → Submit Image ID + inputs
   └─ Local → Execute on FaaS worker (dev only)

4. Proving backend generates proof
   └─ GPU-accelerated proving (SP1/Bonsai)

5. FaaS caches result
   ├─ Store proof with cache key
   └─ Return to user
```

## Storage Strategy Comparison

| Layer | Technology | Purpose | Latency | Cost |
|-------|-----------|---------|---------|------|
| **FaaS Cache** | In-memory LRU | Hot programs & proofs | <1ms | Free (ephemeral) |
| **IPFS** | Decentralized | Permanent program storage | 50-500ms | $0.01/GB/month |
| **Arweave** | Permanent storage | Archival programs | 100-1000ms | One-time: $2/GB |
| **Smart Contract** | On-chain | Program registry | 100-500ms | Gas fees |

## Guest Program Storage: Why IPFS?

### Problem with Current Approaches

**SP1 Network:**
- ✗ Client uploads full ELF (~1-10 MB) with every request
- ✗ Bandwidth-heavy
- ✗ No deduplication

**RISC Zero Bonsai:**
- ✗ Centralized Image ID registry
- ✗ Single point of failure
- ✗ Bonsai controls which programs exist

### Our Solution: Decentralized Registry

**IPFS + Blueprint Contract:**
- ✓ Upload ELF once → get CID
- ✓ Register CID on-chain → verifiable
- ✓ Anyone can fetch from IPFS → decentralized
- ✓ FaaS caches frequently-used programs → fast

## Proof Caching Strategy

### Cache Key Generation

```rust
fn cache_key(program_hash: &str, inputs: &[String], backend: &ZkBackend) -> String {
    let mut hasher = Sha256::new();
    hasher.update(program_hash);
    for input in inputs {
        hasher.update(input.as_bytes());
    }
    hasher.update(format!("{:?}", backend).as_bytes());
    format!("zkproof_{:x}", hasher.finalize())
}
```

### Cache Layers

1. **Memory Cache (L1)**
   - LRU with 1000 entry limit
   - Instant retrieval (<1ms)
   - Cleared on restart

2. **Disk Cache (L2)**
   - Persistent across restarts
   - 10GB size limit
   - ~10ms retrieval

3. **Distributed Cache (L3 - Future)**
   - Shared across FaaS workers
   - Redis/Memcached
   - <50ms retrieval

## Integration with Blueprint Smart Contract

### Solidity Contract (Simplified)

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ZkProgramRegistry {
    struct ProgramMetadata {
        string ipfsCid;         // "Qm..."
        bytes32 elfHash;        // SHA256(ELF binary)
        address author;         // Program creator
        uint256 timestamp;      // Registration time
        string description;     // Human-readable
        string zkvm;            // "SP1" or "RISCZero"
    }

    mapping(bytes32 => ProgramMetadata) public programs;

    event ProgramRegistered(
        bytes32 indexed programHash,
        string ipfsCid,
        address indexed author
    );

    function registerProgram(
        bytes32 programHash,
        string memory ipfsCid,
        bytes32 elfHash,
        string memory description,
        string memory zkvm
    ) external {
        require(programs[programHash].author == address(0), "Program exists");

        programs[programHash] = ProgramMetadata({
            ipfsCid: ipfsCid,
            elfHash: elfHash,
            author: msg.sender,
            timestamp: block.timestamp,
            description: description,
            zkvm: zkvm
        });

        emit ProgramRegistered(programHash, ipfsCid, msg.sender);
    }

    function getProgram(bytes32 programHash)
        external
        view
        returns (ProgramMetadata memory)
    {
        return programs[programHash];
    }
}
```

### Blueprint Integration

```rust
use blueprint_sdk::*;

#[blueprint]
struct ZkFaasService {
    registry: ZkProgramRegistry,
}

#[blueprint_job]
async fn prove(
    program_hash: String,
    inputs: Vec<String>
) -> Result<ZkProof, Error> {
    // 1. Resolve program from on-chain registry
    let metadata = registry.get_program(&program_hash).await?;

    // 2. Fetch ELF from IPFS
    let elf = ipfs_fetch(&metadata.ipfs_cid).await?;

    // 3. Verify ELF integrity
    assert_eq!(sha256(&elf), metadata.elf_hash);

    // 4. Submit to proving network
    let proof = submit_to_sp1_network(elf, inputs).await?;

    Ok(proof)
}
```

## Backend Comparison

| Backend | Latency | Cost | GPU | Decentralized | Status |
|---------|---------|------|-----|---------------|--------|
| **SP1 Local** | 10-60s | Free | Optional | ✓ | ✅ Implemented |
| **SP1 Network** | 5-30s | $PROVE tokens | ✓ | ✓ | 🔄 API Integration |
| **Bonsai** | 5-30s | $/proof | ✓ | ✗ Centralized | 📋 Planned |
| **RISC Zero Local** | 10-60s | Free | Optional | ✓ | 📋 Planned |

## Performance Optimizations

### 1. Proof Deduplication

**Problem:** Multiple users request identical proofs
**Solution:** Cache proofs by `hash(program + inputs + backend)`
**Impact:** 99% cache hit rate for common operations

### 2. Program Caching

**Problem:** Fetching from IPFS is slow (100-500ms)
**Solution:** LRU cache of top 100 programs
**Impact:** <1ms program resolution for hot programs

### 3. Parallel Proving

**Problem:** Single proof request takes 10-30s
**Solution:** Batch multiple proof requests to different workers
**Impact:** Linear scaling with worker count

## Security Considerations

### 1. Program Integrity

- ✓ ELF hash stored on-chain
- ✓ Verification before execution
- ✓ Immutable once registered

### 2. Proof Validity

- ✓ Proofs verified on-chain
- ✓ Public inputs committed in proof
- ✓ Cryptographic guarantees from zkVM

### 3. Denial of Service

- ✓ Rate limiting per user
- ✓ Proof request quotas
- ✓ Resource limits on FaaS workers

## Future Enhancements

1. **GPU Proving Pool**
   - FaaS workers with GPU support
   - Local GPU-accelerated proving
   - Competitive with SP1 Network pricing

2. **Cross-chain Registry**
   - Multi-chain program registry
   - Bridge proofs between chains
   - Unified program namespace

3. **Proof Aggregation**
   - Combine multiple proofs
   - Reduce on-chain verification cost
   - Batch proving for efficiency

4. **TEE Integration**
   - Trusted execution for private inputs
   - Hardware-based confidentiality
   - Attestation of proof generation

## References

- **SP1 zkVM**: https://github.com/succinctlabs/sp1
- **RISC Zero**: https://github.com/risc0/risc0
- **IPFS**: https://ipfs.tech
- **Blueprint SDK**: https://docs.tangle.tools/developers/blueprint-developers
- **FaaS Platform**: https://github.com/tangle-network/faas

## License

MIT OR Apache-2.0
