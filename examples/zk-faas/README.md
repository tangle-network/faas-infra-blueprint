# Zero-Knowledge FaaS Example

Demonstrates ZK proof generation and verification using FaaS execution modes.

## Features

- ✅ ZK proof generation
- ✅ Proof verification
- ✅ Circuit compilation
- ✅ Witness generation
- ✅ Execution modes for performance

## Running

```bash
# Start FaaS gateway server
cargo run --release --package faas-gateway-server

# Run ZK workflow
cargo run --release --package zk-faas-example
```

## Workflow

1. **Circuit Definition**: Define ZK circuit
2. **Witness Generation**: Generate witness from inputs
3. **Proof Generation**: Create ZK proof
4. **Verification**: Verify proof validity

## Use Cases

- Privacy-preserving computation
- Verifiable computation
- Zero-knowledge authentication
- Blockchain applications

## Lines of Code

295 lines - Complete ZK workflow
