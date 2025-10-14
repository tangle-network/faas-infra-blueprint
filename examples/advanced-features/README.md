# Advanced Features Example

Comprehensive demo of FaaS platform capabilities including multi-language execution, caching, forking, and snapshots.

## Prerequisites

- **Docker** - Required for container execution
  - Download from: https://www.docker.com/products/docker-desktop
- **FaaS Gateway Server** - Must be running on port 8080
- **Docker Images** - The following images will be pulled automatically:
  - `alpine:latest`
  - `python:3.11-slim`
  - `node:20-alpine`

## Features Demonstrated

- ✅ Multi-language execution (Python, JavaScript, Bash)
- ✅ Execution forking and branching
- ✅ Snapshot management
- ✅ Intelligent caching
- ✅ Runtime selection (Docker/Firecracker)
- ✅ Resource limits (memory, CPU, timeout)
- ✅ Client-side metrics
- ✅ Health monitoring

## Known Limitations

- **Fork Execution on macOS**: CRIU (Checkpoint/Restore In Userspace) is not available on macOS. Fork execution uses a simplified implementation that runs in fresh containers instead of using checkpoints. This maintains API compatibility while working cross-platform.
- **Firecracker**: Only available on Linux with KVM support

## Running

```bash
# 1. Start FaaS gateway server (in one terminal)
cargo run --release --package faas-gateway-server

# 2. Wait for gateway to start (look for "listening on 0.0.0.0:8080")

# 3. Run the example (in another terminal)
cargo run --release --package advanced-features
```

## Example Output

```
🚀 FaaS Platform Advanced Features Demo

1. Quick command execution:
   Output: Hello from FaaS!

2. Python execution:
   Python output: {
     "result": 42,
     "status": "computed",
     "python_version": "3.11.x"
   }

3. JavaScript execution:
   JavaScript output: {
     "result": 84,
     "timestamp": "2024-10-03T...",
     "node_version": "v20.x.x"
   }

4. Forked execution:
   Fork result: Forked execution completed

...
```

## Key Concepts

### Forking
Branch execution from a base state - perfect for A/B testing or parallel exploration:

```rust
let base = client.execute(/* base config */).await?;
let fork = client.fork_execution(&base.request_id, "new command").await?;
```

### Caching
Automatic result caching based on command + image + env:

```rust
let result = client.run_cached("echo 'cached'", "alpine:latest").await?;
```

### Snapshots
Create reusable environment snapshots:

```rust
let snapshot_id = client.create_snapshot(/* config */).await?;
let result = client.execute_from_snapshot(snapshot_id).await?;
```

## Lines of Code

112 lines showing the full SDK capabilities.
