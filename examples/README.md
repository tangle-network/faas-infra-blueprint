# FaaS Platform Examples

Production-ready services built on the FaaS platform.

## Quick Start

```bash
# Build all examples
cargo build --workspace --release

# Run specific example
cargo run -p gpu-service-example
cargo run -p agent-branching-example
cargo run -p zk-faas-example
cargo run -p remote-dev-example
```

## Examples

### GPU Service
ML model serving with instant warm starts.
```bash
cargo run -p gpu-service-example
# Output: 30s cold start → <100ms warm (300x speedup)
```

### Agent Branching
Parallel exploration for AI agents.
```bash
cargo run -p agent-branching-example
# Output: 550s setup → 10s restore (50x speedup)
```

### ZK-FaaS
Zero-knowledge proof generation.
```bash
cargo run -p zk-faas-example
# Output: Private ML inference, compliance proofs
```

### Remote Development
Browser-based development environments.
```bash
cargo run -p remote-dev-example
# Jupyter: http://localhost:8888
# VSCode: http://localhost:3000
# Desktop: http://localhost:6080
# Bun: http://localhost:3000
```

## Requirements

- Docker
- Rust nightly (`rustup install nightly`)

## Testing

```bash
./examples/test_examples.sh
```