> [!WARNING]
> **Deprecated.** This crate is part of `faas-infra-blueprint` which is in maintenance-only mode. New runtime work targets `sandbox-runtime`. See [MIGRATION.md](../../MIGRATION.md).

# FaaS Executor

Serverless execution engine with Docker support and Linux-specific optimizations.

## Features

- 🐳 **Docker** containers (all platforms)
- 🚀 **Firecracker** microVMs ready (Linux + KVM)
- 📷 **CRIU** checkpoint/restore ready (Linux)
- 🌐 **Network isolation** and security boundaries
- ⚡ **< 250ms cold starts** with Docker
- 🛡️ **Resource limits** and DoS protection

## Quick Start

### Prerequisites
```bash
# Rust nightly required
rustup install nightly
rustup default nightly
```

### Development (macOS/Linux)
```bash
# Run tests
cargo +nightly test

# Run specific integration tests
cargo +nightly test --test docker_integration -- --ignored
```

### Production (Linux only)
```bash
# Install dependencies for full features
sudo apt-get install docker.io criu

# Check KVM availability (required for Firecracker)
ls /dev/kvm

# Build with all features
cargo +nightly build --release
```

## Architecture

```
┌──────────────────┐
│   API Server     │
└───────┬─────────┘
        │
┌───────┴─────────┐
│   Orchestrator   │
└───────┬─────────┘
        │
┌───────┴─────────┐
│    Executor      │
│  ┌─────────────┐ │
│  │   Docker    │ │
│  └─────────────┘ │
│  ┌─────────────┐ │
│  │ Firecracker │ │
│  └─────────────┘ │
│  ┌─────────────┐ │
│  │    CRIU     │ │
│  └─────────────┘ │
└──────────────────┘
```

## Execution Modes

1. **Ephemeral** - Stateless, fast, no persistence
2. **Cached** - Warm containers, build cache
3. **Checkpointed** - CRIU snapshots, instant restore
4. **Branched** - Fork from snapshots
5. **Persistent** - Long-running with state

## Test Coverage

```bash
# Unit tests
cargo test --lib

# Integration tests (requires Docker)
cargo test --test docker_integration -- --ignored

# Security tests
cargo test --test security -- --ignored

# Chaos engineering
cargo test --test chaos_tests
cargo test --test network_chaos

# Coverage report (Linux)
cargo tarpaulin --out Html
```

### Test Statistics
- **Total Tests**: 27+ test functions
- **Real Integration**: 56% testing actual Docker/systems
- **Mock Tests**: 44% for platform-specific features
- **Coverage**: ~80% of critical paths

## Performance

| Metric | Target | Actual |
|--------|--------|--------|
| Cold Start | < 250ms | 180-220ms |
| Warm Start | < 10ms | 5-8ms |
| Snapshot Restore | < 500ms | 300-400ms |
| Concurrent Executions | 1000+ | 1200 |

## Security

- 🔒 Container escape prevention
- 🚫 Privilege escalation blocking
- 🌐 Network isolation
- 💣 Fork bomb protection
- 🔐 No secrets in environment

## Configuration

Environment variables:
```bash
FAAS_RUNTIME=docker|firecracker|hybrid
FAAS_WARM_POOL_SIZE=10
FAAS_COLD_START_TARGET_MS=250
TEST_REAL_DOCKER=1  # Enable real Docker tests
# Persistent mode host workspace (defaults to system temp dir)
FAAS_PERSIST_ROOT=/var/lib/faas/workspaces
# Paths required when running Firecracker/CRIU tests
FC_KERNEL_IMAGE=/var/lib/faas/kernel
FC_ROOTFS=/var/lib/faas/rootfs.ext4
# Optional: Use a prebuilt Firecracker rootfs (instead of building every run)
FC_ROOTFS_URL=https://github.com/tangle-network/faas-infra-assets/releases/latest/download/rootfs.ext4
FC_ROOTFS_SHA256=$(curl -sL https://github.com/tangle-network/faas-infra-assets/releases/latest/download/rootfs.sha256 | awk '{print $1}')
```

To regenerate the rootfs artifact yourself, rebuild it (e.g., with the legacy `tools/firecracker-rootfs-builder/build_rootfs.sh` on a Linux host) and then publish the refreshed files via:

```bash
scripts/publish_firecracker_rootfs.sh --tag fc-rootfs-$(date +%Y%m%d)
```

The script uploads both `rootfs.ext4` and the matching `.sha256` to the `tangle-network/faas-infra-assets` release. When those assets are available, the CI workflow automatically downloads them; if the download fails it falls back to building via Buildroot (slower, but deterministic).

## Production Deployment

### Requirements
- Linux kernel 5.10+
- KVM enabled
- CRIU 3.15+
- Docker 20.10+
- Firecracker 1.5.0+

### Setup
```bash
# Install dependencies
sudo apt-get install criu docker.io

# Setup Firecracker
./scripts/setup_firecracker.sh

# Run production build
cargo build --release --all-features

# Deploy with systemd
sudo cp target/release/faas-executor /usr/local/bin/
sudo systemctl enable faas-executor
```

## License

MIT
