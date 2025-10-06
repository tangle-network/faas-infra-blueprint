# faas-zk-prover

Production ZK proof generation microservice with SP1 zkVM.

## Overview

HTTP server providing zero-knowledge proof generation via REST API. Isolated from workspace due to sp1-sdk/blueprint-sdk native library conflict (both link to c-kzg).

## Architecture

```
Client (faas-zkvm) → HTTP → faas-zk-prover (SP1 zkVM)
                             ├─ POST /v1/prove
                             └─ GET  /health
```

## Build

```bash
cd crates/faas-zk-prover
cargo build --release
```

**Note**: First build ~10-15 min (compiles SP1 zkVM stack). Requires `sp1up` toolchain.

## Run

```bash
cargo run --release
# Server starts on http://0.0.0.0:8081
```

## API

### POST /v1/prove

Generate ZK proof using SP1 local prover.

**Request:**
```json
{
  "program": "fibonacci",
  "public_inputs": ["10"],
  "private_inputs": []
}
```

**Response:**
```json
{
  "proof_id": "a3f2...",
  "program": "fibonacci",
  "public_inputs": ["10"],
  "proof_data": "base64_encoded_proof...",
  "backend": "SP1 Local",
  "proving_time_ms": 3245
}
```

### GET /health

Health check endpoint.

**Response:** `"ok"`

## Client Usage

Use `faas-zkvm` library from workspace:

```rust
use faas_zkvm::ZkProverClient;

let client = ZkProverClient::new("http://localhost:8081");
let proof = client.prove("fibonacci", vec!["10".to_string()], vec![]).await?;
```

## Guest Programs

Located in `guest-programs/`:
- **fibonacci/** - Fibonacci sequence computation proof
- **hash-preimage/** - Hash preimage knowledge proof

Add new programs via `build.rs`:

```rust
sp1_build::build_program_with_args(
    "guest-programs/my-program",
    sp1_build::BuildArgs::default(),
);
```

## Prerequisites

```bash
# Install SP1 toolchain
curl -L https://sp1.succinct.xyz | bash
sp1up

# Verify
rustc +succinct --version  # Should show 1.88.0-dev
```

## Configuration

Runs in release mode only (SP1 requirement). Set `RUST_LOG=info` for logging.

## Integration Test

```bash
# Terminal 1: Start server
cargo run --release --package faas-zk-prover

# Terminal 2: Run tests
cargo test -p faas-zkvm -- --ignored --nocapture
```

## Production Deployment

```yaml
# docker-compose.yml
services:
  zk-prover:
    build:
      context: .
      dockerfile: crates/faas-zk-prover/Dockerfile
    ports:
      - "8081:8081"
    environment:
      - RUST_LOG=info
```

## Resources

- **SP1 Docs**: https://docs.succinct.xyz/
- **Prover Network**: https://prover.succinct.xyz
- **GitHub**: https://github.com/succinctlabs/sp1
