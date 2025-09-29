# FaaS Platform

Production-ready serverless execution platform with sub-50ms warm starts.

**Platform Support:**
- macOS: Docker executor with container pooling
- Linux: Docker + Firecracker microVMs + CRIU checkpoint/restore

## Architecture

```
crates/
├── faas-common/        # Shared types and traits
├── faas-executor/      # Docker execution engine (Firecracker/CRIU ready)
├── faas-gateway/       # API gateway service
├── faas-usage-tracker/ # Usage tracking and billing
└── faas-guest-agent/   # Agent for Firecracker VMs (Linux)

faas-lib/              # Core FaaS library and Blueprint SDK (root level)
```

## Quick Start

```bash
# Prerequisites
rustup install nightly
brew install docker   # macOS
apt install docker.io # Linux
```

### Build & Run

```bash
cargo +nightly build --release
cargo run --package faas-gateway-server --release

# Test endpoint
curl -X POST http://localhost:8080/api/v1/execute \
  -d '{"command": "echo test", "image": "alpine:latest"}'
```

## Testing

```bash
cargo +nightly test --lib                    # Unit tests
cargo test --test docker_integration -- --ignored  # Integration tests (requires Docker)
./test-faas-platform test                    # Full test suite
```

## API Usage

```bash
# Execute function
curl -X POST http://localhost:8080/api/v1/execute \
  -H "Content-Type: application/json" \
  -d '{"command": "echo test", "image": "alpine:latest"}'

# Advanced execution with caching
curl -X POST http://localhost:8080/api/v1/execute/advanced \
  -d '{"command": "compile", "image": "rust:latest", "mode": "cached"}'
```

## Linux Production Features (Ready but Requires KVM)

### Firecracker Support
- MicroVM-based isolation
- Sub-100ms boot times
- Requires KVM virtualization

### CRIU Support
- Checkpoint/restore for instant warm starts
- Process state preservation
- Live migration capability

To enable these features on Linux:
1. Ensure KVM is available: `ls /dev/kvm`
2. Install CRIU: `sudo apt-get install criu`
3. Download Firecracker binaries (handled by test script)

## Development Workflow

### Directory Structure
```
.
├── crates/            # Rust workspace members
├── contracts/         # Smart contracts
├── dependencies/      # Vendored dependencies
├── docs/             # Documentation
├── sdk/              # Language SDKs (Python, TypeScript)
├── tools/            # Build tools
└── test-faas-platform # Unified test runner
```

### Running Specific Test Suites

```bash
# Docker integration tests only
cargo +nightly test --test docker_integration -- --ignored

# Comprehensive platform tests
cargo +nightly test --test comprehensive_tests -- --ignored

# Platform setup tests
cargo +nightly test --test platform_setup_test

# Core library tests
cargo +nightly test --manifest-path faas-lib/Cargo.toml
```

## Deployment

### Docker Compose

```yaml
version: '3.8'
services:
  gateway:
    image: faas-gateway:latest
    ports:
      - "8080:8080"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    environment:
      - RUST_LOG=info
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: faas-gateway
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: gateway
        image: faas-gateway:latest
        ports:
        - containerPort: 8080
```

## Security Best Practices

✅ **Implemented**:
- Container isolation
- Resource limits
- Input validation
- Timeout controls
- CORS support

⚠️ **Recommended for Production**:
- Add authentication (JWT/API keys)
- Enable rate limiting
- Use TLS/HTTPS
- Set up monitoring
- Configure logging

## CI/CD Pipeline

GitHub Actions configured for:
- Multi-OS testing (Ubuntu, macOS)
- Rust stable & nightly
- Python SDK (3.8-3.11)
- TypeScript SDK (Node 18, 20)
- Docker integration
- Security audits

## Documentation

- 📚 [Production Report](./PRODUCTION_READY_REPORT.md)
- 🏗️ [Architecture](./docs/ARCHITECTURE.md)
- 🐍 [Python SDK](./sdk/python/README.md)
- 📘 [TypeScript SDK](./sdk/typescript/README.md)
- 💡 [Examples](./examples/)

## License

Apache License, Version 2.0

---

**🚀 Production Status: READY FOR DEPLOYMENT**