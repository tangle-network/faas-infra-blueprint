# FaaS Platform

High-performance serverless execution platform with Docker container support. Prepared for Firecracker microVMs and CRIU checkpoint/restore on Linux.

## Current Status

**Platform Support:**
- ✅ **macOS**: Docker executor only
- ✅ **Linux**: Docker, with Firecracker and CRIU ready (requires KVM)

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

### Prerequisites

```bash
# Install Rust nightly (required for dependencies)
rustup install nightly
rustup default nightly

# macOS
brew install docker

# Linux
sudo apt-get install docker.io
# For Firecracker support (Linux only):
# - Enable KVM virtualization in BIOS
# - Install CRIU: sudo apt-get install criu
```

### Build & Test

```bash
# Build the project
cargo +nightly build --release

# Run tests
./test-faas-platform test

# Check platform capabilities
./test-faas-platform setup
```

## Testing

The platform uses a consolidated test runner that adapts to your OS:

```bash
# Run all tests appropriate for your platform
./test-faas-platform test

# Platform capability check
./test-faas-platform setup

# Clean test artifacts
./test-faas-platform clean
```

### Test Categories

- **Unit Tests**: `cargo +nightly test --lib`
- **Docker Integration**: Works on all platforms
- **Comprehensive Tests**: Full platform capability tests
- **Linux-only Tests**: Firecracker and CRIU tests (skipped on macOS)

## Execution via Docker

Currently, the platform uses Docker for container execution:

```rust
use faas_executor::DockerExecutor;
use faas_common::{SandboxConfig, SandboxExecutor};
use bollard::Docker;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let docker = Arc::new(Docker::connect_with_defaults().unwrap());
    let executor = DockerExecutor::new(docker);

    let result = executor.execute(SandboxConfig {
        function_id: "test".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "Hello FaaS".to_string()],
        env_vars: None,
        payload: vec![],
    }).await.unwrap();

    println!("Output: {:?}", result.response);
}
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

## Performance Targets

| Metric | Target | Current (Docker) |
|--------|--------|-----------------|
| Cold Start | <250ms | ~200ms |
| Warm Start | <10ms | ~50ms |
| Concurrent | 1000+ | 100+ (Docker limited) |

*Note: Full performance targets achievable with Firecracker on Linux*

## Troubleshooting

### macOS Limitations
- KVM not available - Firecracker won't work
- Use Docker executor for all testing
- For full capabilities, use Linux with KVM

### Docker Issues
- Ensure Docker daemon is running
- Check Docker socket permissions
- Verify no conflicting containers with `docker ps`

### Test Failures
- Run `./test-faas-platform clean` to reset
- Check Docker is running: `docker version`
- Review logs with `--nocapture` flag

## Contributing

1. Fork the repository
2. Create a feature branch
3. Run tests: `./test-faas-platform test`
4. Submit a pull request

## License

Apache License, Version 2.0