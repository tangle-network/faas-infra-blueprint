# FaaS Platform Examples - Production Guide

## What Actually Works

### ✅ Fully Functional (Cross-Platform)

#### 1. **Docker-based Execution** (`quickstart`)
- **Status**: 100% functional
- **Platform**: macOS, Linux, Windows
- **Features**:
  - Container creation and execution
  - Stdin/stdout handling
  - Environment variables
  - Resource limits

#### 2. **Basic Model Loading** (`gpu-service`)
- **Status**: Docker execution works
- **Platform**: macOS, Linux
- **Features**:
  - Loads real PyTorch models in containers
  - Measures actual execution time
  - Docker commit for pseudo-snapshots

### ⚠️ Linux-Only Features

#### 1. **CRIU Checkpointing**
- **Location**: `crates/faas-executor/src/criu/`
- **Status**: Fully implemented, requires Linux + CRIU binary
- **Features**:
  - Real memory snapshots
  - Process tree checkpoint/restore
  - Sub-second warm starts

```bash
# On Linux:
sudo apt-get install criu
cargo test --package faas-executor --test criu_integration -- --ignored
```

#### 2. **Firecracker MicroVMs**
- **Location**: `crates/faas-executor/src/firecracker/`
- **Status**: Implemented, requires KVM
- **Features**:
  - True VM snapshots
  - Memory ballooning
  - Device hotplug

### 🔴 Currently Simulated

#### 1. **Agent Branching** (`agent-branching`)
**Issue**: Claims instant restore but doesn't use snapshots
**Fix Required**:
```rust
// Instead of:
let result = executor.execute(config).await?;

// Should use:
use faas_executor::platform::snapshot::SnapshotStore;
let store = SnapshotStore::new().await?;
let snapshot = store.create_criu_snapshot(&container_id).await?;
let restored = store.restore_snapshot(&snapshot.id).await?;
```

#### 2. **Remote Dev Environments** (`remote-dev`)
**Issue**: Just echoes URLs, doesn't start services
**Fix Required**: Use actual images with services:
```rust
// Jupyter:
source: "jupyter/datascience-notebook:latest"
command: vec!["start-notebook.sh"]

// VSCode:
source: "gitpod/openvscode-server:latest"
command: vec!["sh", "-c", "exec /home/.openvscode-server/bin/openvscode-server --host 0.0.0.0"]
```

## How to Make Examples Production-Ready

### Step 1: Use Existing Library Features

The library already has:
- CRIU manager: `crates/faas-executor/src/criu/`
- Firecracker support: `crates/faas-executor/src/firecracker/`
- Snapshot system: `crates/faas-executor/src/platform/snapshot.rs`
- Container pooling: `crates/faas-executor/src/performance/container_pool.rs`

### Step 2: Platform-Specific Code

```rust
use faas_executor::criu::CriuManager;

#[cfg(target_os = "linux")]
async fn create_snapshot(container_id: &str) -> Result<Snapshot> {
    let criu = CriuManager::new()?;
    criu.checkpoint(container_id).await
}

#[cfg(not(target_os = "linux"))]
async fn create_snapshot(container_id: &str) -> Result<Snapshot> {
    // Fallback to Docker commit
    docker.commit_container(container_id).await
}
```

### Step 3: Real Service Images

Instead of `alpine:latest` with echo commands, use:

| Service | Image | Command |
|---------|-------|---------|
| PostgreSQL | `postgres:15` | Default entrypoint |
| Redis | `redis:7` | Default entrypoint |
| Jupyter | `jupyter/base-notebook` | `start-notebook.sh` |
| Node.js | `node:20` | `node server.js` |
| Python | `python:3.11` | `python app.py` |

## Testing on Linux

```bash
# Install CRIU
sudo apt-get install criu

# Test CRIU functionality
cargo test --package faas-executor --features criu

# Test Firecracker (requires KVM)
cargo test --package faas-executor --features firecracker

# Run GPU example with real CRIU snapshots
sudo cargo run --example gpu-service --features criu
```

## Production Deployment

### For GPU Workloads
1. Use NVIDIA Container Toolkit
2. Enable CRIU with `--tcp-established` for network connections
3. Use persistent volumes for model storage

### For CI/CD
1. Use the container pool for warm containers
2. Cache build artifacts in volumes
3. Use layered Docker images efficiently

### For Data Pipelines
1. Use the existing `SandboxExecutor` trait
2. Chain executions with payload passing
3. Monitor with the metrics collector

## Key Takeaways

1. **The library is production-ready** - It has real CRIU, Firecracker, and snapshot support
2. **Examples need updating** - Most examples don't use the actual library features
3. **Platform matters** - CRIU and Firecracker require Linux
4. **Docker fallback works** - Docker commit/save/load provides cross-platform snapshots

## Next Steps

1. Update examples to use `faas_executor::criu::CriuManager` directly
2. Add conditional compilation for Linux-specific features
3. Use real service containers instead of `alpine` with echo
4. Add integration tests that verify actual functionality
5. Document which features work on which platforms