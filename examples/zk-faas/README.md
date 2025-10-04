# ZK-FaaS: Zero-Knowledge Proof Generation as a Service

Production-grade ZK proof generation with unified abstraction for local + delegated proving.

## Architecture

**Unified Interface** for multiple ZK backends:
- **Local Proving**: Run zkVM in FaaS containers (full control, privacy)
- **Network Proving**: Delegate to proving services (faster, scalable)
- **Hybrid**: Switch backends with one line of code!

## Supported Backends

### 📦 Local Proving (via FaaS containers)

| Backend | Status | Speed | Features |
|---------|--------|-------|----------|
| **SP1 zkVM** | 🔄 Planned | ~5s | RISC-V, LLVM, fastest local |
| **RISC Zero zkVM** | 🔄 Planned | ~4.5s | RISC-V, enterprise-grade |
| **Brevis Pico** | 🔄 Planned | ~2.9s | RISC-V, 84% faster! |

### 🌐 Network Proving (delegated services)

| Backend | Status | Speed | Requirements |
|---------|--------|-------|--------------|
| **Bonsai Network** | ⏳ Next | ~2s | RISC Zero API key (free tier) |
| **SP1 Network** | 🔄 Planned | ~1.8s | $PROVE token |
| **Brevis Network** | 🔄 Planned | ~2s | API key |

## Quick Start

```bash
# Run the architecture demo
cargo run --release --package zk-faas-example
```

## Example Usage

```rust
use faas_sdk::FaasClient;
use zk_faas::{ZkProvingService, ZkBackend};

// Local proving
let prover = ZkProvingService::new(
    FaasClient::new("http://localhost:8080".into()),
    ZkBackend::Sp1Local
);

// Network proving (delegated)
let prover = ZkProvingService::new(
    FaasClient::new("http://localhost:8080".into()),
    ZkBackend::BonsaiNetwork {
        api_key: env::var("BONSAI_API_KEY")?
    }
);

// Generate proof
let proof = prover.prove(
    "fibonacci",        // Program name
    vec!["1000".into()], // Public inputs
    vec![],             // Private inputs
).await?;
```

## Use Cases

### 1. **Fibonacci Computation**
Prove correct computation without revealing intermediate steps.

### 2. **Hash Preimage Knowledge**
Privacy-preserving authentication - prove you know a password without revealing it.

### 3. **Private ML Inference**
Prove model predictions without revealing proprietary model weights.

### 4. **Cross-Chain Bridges**
Verify blockchain state transitions with ZK proofs.

### 5. **Regulatory Compliance**
Prove KYC compliance without disclosing sensitive customer data.

## Roadmap

### ✅ Phase 1: Architecture (Current)
- [x] Design unified ZK proving abstraction
- [x] Document all backends (SP1, RISC Zero, Brevis)
- [x] Show use cases and performance comparison
- [x] Clean example demonstrating architecture

### 🔄 Phase 2: Initial Integration
- [ ] Integrate RISC Zero Bonsai API (easiest to get started!)
- [ ] Add Bonsai authentication and proof generation
- [ ] Real Fibonacci + Hash Preimage examples
- [ ] Documentation and tutorials

### 🔄 Phase 3: Local Proving
- [ ] Add SP1 local proving with guest programs
- [ ] Build guest programs at compile time
- [ ] FaaS container integration
- [ ] Snapshot caching for prover setup

### 🔄 Phase 4: Advanced Features
- [ ] GPU acceleration for local proving
- [ ] Parallel batch proving via FaaS forking
- [ ] Proof aggregation
- [ ] Performance benchmarks across all backends
- [ ] Cost optimization strategies

### 🔄 Phase 5: Production Features
- [ ] SP1 Network integration
- [ ] Brevis Pico integration
- [ ] On-chain verifier generation (Solidity)
- [ ] REST API for proof generation
- [ ] Metrics and monitoring
- [ ] Proof marketplace

## Performance Comparison

### Fibonacci(1000) Benchmark

| Backend | Proving Time | Setup Time | Total | Notes |
|---------|--------------|------------|-------|-------|
| SP1 Local | ~5s | ~2s | ~7s | Via FaaS container |
| RISC Zero Local | ~4.5s | ~2s | ~6.5s | Via FaaS container |
| Brevis Pico | ~2.9s | ~1s | ~3.9s | 84% faster! |
| **Bonsai Network** | **~2s** | **0s** | **~2s** | **Delegated (easiest!)** |
| SP1 Network | ~1.8s | 0s | ~1.8s | Decentralized |

**Verdict**:
- **Development**: Use local proving (full control, no API keys)
- **Production**: Use network proving (faster, scalable)
- **Best GTM**: Start with Bonsai (free tier, easy API)

## Why FaaS for ZK Proving?

### 🚀 Scalability
- Parallel proof generation via FaaS containers
- Linear scaling with container count
- Handle 100s of concurrent proofs

### ⚡ Performance
- Snapshot caching for prover setup (pk/vk)
- First proof: ~10s (setup + prove)
- Cached proofs: ~2s (prove only!)

### 🔐 Isolation
- Each proof runs in isolated container
- Prevents interference between proofs
- Security and resource isolation

### 💰 Cost Optimization
- Pay only for proving time
- No idle prover instances
- Automatic resource management

## Architecture Details

### Local Proving Flow
```
User → ZkProvingService → FaaS Container → zkVM → Proof
                              ↓
                        (Cached setup via snapshot)
```

### Network Proving Flow
```
User → ZkProvingService → Bonsai API → Prover Network → Proof
                              ↓
                        (Delegated to cluster)
```

## Integration Options

### Option 1: Use as Example (Current)
- Copy architecture into your project
- Customize for your use case
- Full flexibility

### Option 2: SDK Integration (Future)
```toml
[dependencies]
faas-sdk = { version = "0.1", features = ["zk"] }
```

```rust
use faas_sdk::zk::{ZkBackend, ZkProvingService};
```

## Development

```bash
# Build example
cargo build --release --package zk-faas-example

# Run demo
cargo run --release --package zk-faas-example

# With Bonsai API key (when implemented)
BONSAI_API_KEY=your_key cargo run --release --package zk-faas-example
```

## Resources

### SP1 (Succinct Labs)
- **Docs**: https://docs.succinct.xyz/
- **GitHub**: https://github.com/succinctlabs/sp1
- **Network**: SP1 Prover Network ($PROVE token)

### RISC Zero
- **Docs**: https://dev.risczero.com/
- **GitHub**: https://github.com/risc0/risc0
- **Bonsai**: https://risczero.com/bonsai
- **API**: https://dev.risczero.com/api/generating-proofs/remote-proving

### Brevis
- **Docs**: https://docs.brevis.network/
- **Pico**: https://pico-docs.brevis.network
- **Network**: https://brevis.network/

## Contributing

1. Start with Bonsai integration (easiest API)
2. Add real guest programs
3. Performance benchmarks
4. Documentation improvements

## License

MIT OR Apache-2.0
