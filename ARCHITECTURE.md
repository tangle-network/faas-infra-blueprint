# FaaS Platform Architecture

## Overview

This is a **general-purpose serverless execution platform** that supports multiple use cases through a unified set of features. The platform is NOT specifically designed for any single use case—instead, it provides general capabilities that can be composed for different workloads.

## Core Philosophy

**General-Purpose First**: Every feature in this platform is designed to be useful across multiple use cases. We avoid use-case-specific code in the core.

**Composable Features**: Users combine execution modes, runtimes, and APIs to build their specific workflows.

## Supported Use Cases

| Use Case | Features Leveraged | Example |
|----------|-------------------|---------|
| **Ephemeral Functions** (Lambda-style) | Ephemeral mode, Docker runtime, Smart caching | API request handlers, webhooks |
| **CI/CD Pipelines** | Persistent mode, WebSocket streaming, Shared dependencies | Build logs, test runners, security scanning |
| **ML Training** | Persistent mode, Checkpointing, S3 storage | Save model checkpoints, resume training |
| **Vibecoding Platforms** | Persistent mode, WebSocket streaming, Shared dependencies | AI agent dev environments, code streaming |
| **Interactive Shells** | Persistent mode, WebSocket bidirectional | Terminal multiplexing, remote shells |
| **Data Pipelines** | Forking, Checkpointing, S3 storage | ETL workflows, data transformations |

## Architecture Layers

```
┌──────────────────────────────────────────────────────────────────┐
│                      SDKs (TypeScript/Python/Rust)              │
│  → Language-specific, zero config, simple API                   │
└────────────────────────┬─────────────────────────────────────────┘
                         │ HTTP/WebSocket
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│                    FaaS Gateway Server                           │
│  → Routing and protocol handling                                 │
│  → HTTP REST API for execution                                   │
│  → WebSocket API for streaming                                   │
│  → Metrics and monitoring                                        │
└────────────────────────┬─────────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Platform Executor                             │
│  → Execution mode selection (ephemeral, persistent, etc.)       │
│  → Runtime selection (Docker, Firecracker, Auto)                │
│  → Request routing to appropriate backend                        │
└────────────────────────┬─────────────────────────────────────────┘
                         │
              ┌──────────┴───────────┐
              │                      │
              ▼                      ▼
┌─────────────────────────┐  ┌──────────────────────┐
│   Docker Executor       │  │  Firecracker Executor│
│  → Container management │  │  → MicroVM management│
│  → Shared volumes       │  │  → Hardware isolation│
│  → Fast iteration       │  │  → Production ready  │
└────────┬────────────────┘  └──────┬───────────────┘
         │                          │
         ▼                          ▼
┌──────────────────────────────────────────────────────────────────┐
│                      Storage Manager                             │
│  → Content-addressed blob storage (SHA256)                      │
│  → Local NVMe cache + S3 replication                            │
│  → Automatic deduplication                                       │
│  → Smart compression (Zstd/LZ4)                                  │
└──────────────────────────────────────────────────────────────────┘
```

## Execution Modes

The platform supports 5 execution modes, each serving different use cases:

### 1. Ephemeral Mode
**Use Cases**: Lambda-style functions, webhooks, API handlers

**Behavior**:
- Container destroyed after execution
- No state preservation
- Fastest mode for simple workloads

**Example**:
```rust
client.execute()
    .mode(ExecutionMode::Ephemeral)
    .command("echo 'Hello'")
    .send().await?;
```

### 2. Cached Mode
**Use Cases**: Repeated executions with same input, idempotent operations

**Behavior**:
- Results cached by content hash
- Subsequent identical requests return cached result
- Automatic cache invalidation

**Example**:
```rust
// First execution: runs in container
client.execute()
    .mode(ExecutionMode::Cached)
    .command("expensive_computation")
    .send().await?;

// Second execution: returns cached result (< 10ms)
client.execute()
    .mode(ExecutionMode::Cached)
    .command("expensive_computation")
    .send().await?;
```

### 3. Checkpointed Mode
**Use Cases**: Long-running workflows, ML training, resumable tasks

**Behavior**:
- Snapshots created at completion
- Can restore and resume from checkpoint
- Stored in content-addressed storage

**Example**:
```rust
// Create checkpoint
let result = client.execute()
    .mode(ExecutionMode::Checkpointed)
    .command("train_model")
    .send().await?;

// Resume from checkpoint later
client.execute()
    .mode(ExecutionMode::Checkpointed)
    .checkpoint(result.snapshot_id)
    .command("continue_training")
    .send().await?;
```

### 4. Branched Mode
**Use Cases**: A/B testing, parallel experimentation, workflow forking

**Behavior**:
- Fork from parent execution
- Multiple variants run in parallel
- Compare results

**Example**:
```rust
// Create parent execution
let parent = client.execute()
    .command("setup_environment")
    .send().await?;

// Fork into variant A
let variant_a = client.fork()
    .parent(parent.id)
    .command("run_algorithm_v1")
    .send().await?;

// Fork into variant B
let variant_b = client.fork()
    .parent(parent.id)
    .command("run_algorithm_v2")
    .send().await?;
```

### 5. Persistent Mode
**Use Cases**: Vibecoding, CI/CD, interactive shells, long-lived services

**Behavior**:
- Container stays alive indefinitely
- Supports WebSocket streaming
- Interactive command execution
- Manual lifecycle management

**Example**:
```rust
// Start persistent container
let container = client.execute()
    .mode(ExecutionMode::Persistent)
    .command("sleep infinity")
    .send().await?;

// Connect via WebSocket
let ws = client.stream(container.id).await?;

// Send commands interactively
ws.send(Command::Exec { command: "npm install" }).await?;
ws.send(Command::Exec { command: "npm test" }).await?;

// Stream output in real-time
while let Some(event) = ws.next().await {
    match event {
        StreamEvent::Stdout { data } => println!("{}", data),
        StreamEvent::Exit { code } => break,
        _ => {}
    }
}
```

## WebSocket Streaming API

The WebSocket API provides **general-purpose bidirectional streaming** for ANY container.

### Event Types (Container → Client)

```typescript
type StreamEvent =
  | { type: 'stdout', data: string }
  | { type: 'stderr', data: string }
  | { type: 'exit', code: number }
  | { type: 'file_event', path: string, event: string }
  | { type: 'process_event', pid: number, command: string, event: string }
  | { type: 'custom', name: string, data: any }
  | { type: 'heartbeat' }
```

### Command Types (Client → Container)

```typescript
type StreamCommand =
  | { type: 'stdin', data: string }
  | { type: 'exec', command: string }
  | { type: 'get_state' }
  | { type: 'checkpoint', name?: string }
  | { type: 'stop' }
```

### Use Case Examples

#### Vibecoding: AI Agent Streaming Code
```javascript
const ws = new WebSocket(`ws://localhost:8080/api/v1/containers/${containerId}/stream`);

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  if (data.type === 'file_event') {
    // AI agent created/modified a file
    updateEditorUI(data.path, data.event);
  }

  if (data.type === 'stdout') {
    // Stream build logs to UI
    appendToTerminal(data.data);
  }
};

// Send command to AI agent
ws.send(JSON.stringify({
  type: 'exec',
  command: 'npm run build'
}));
```

#### CI/CD: Live Build Logs
```python
async with client.stream(container_id) as ws:
    async for event in ws:
        if event.type == 'stdout':
            print(f"[BUILD] {event.data}")
        elif event.type == 'exit':
            sys.exit(event.code)
```

#### ML Training: Live Metrics
```rust
let mut ws = client.stream(container_id).await?;

while let Some(event) = ws.next().await? {
    match event {
        StreamEvent::Custom { name, data } if name == "metrics" => {
            let loss: f64 = data["loss"].as_f64()?;
            let accuracy: f64 = data["accuracy"].as_f64()?;
            update_dashboard(loss, accuracy);
        }
        _ => {}
    }
}
```

## Shared Dependency Caching

The platform implements **Replit-style shared dependency caching** where dependencies are cached in persistent Docker volumes and mounted into ALL containers.

### Implementation

Located in `crates/faas-executor/src/executor.rs:634-702`:

```rust
async fn initialize_dependency_volumes(&self) -> anyhow::Result<()> {
    let volumes = vec![
        ("cargo-registry", "/usr/local/cargo/registry"),
        ("npm-cache", "/root/.npm"),
        ("pip-cache", "/root/.cache/pip"),
        ("go-mod-cache", "/go/pkg/mod"),
    ];

    for (name, mount_path) in volumes {
        // Create persistent volume
        docker.create_volume(name).await?;

        // Mount into ALL containers
        container_config.mounts.push(Mount {
            source: name,
            target: mount_path,
            type: MountType::Volume,
        });
    }
}
```

### Benefits

- **Zero cold starts for dependencies**: First `npm install` downloads packages, subsequent containers use cached packages instantly
- **Shared across all users**: Single `npm-cache` volume serves all containers
- **Storage efficient**: Dependencies stored once, not duplicated per container
- **Cross-project reuse**: Packages from project A automatically available to project B

## Multi-Node Scaling Strategy

For scaling to 1M+ users (e.g., vibecoding platform), the architecture supports distributed deployment:

### Approach 1: Blockchain-Coordinated (Tangle Blueprint)

```
┌─────────────────────────────────────────────────────────┐
│            Tangle Blockchain (Coordination)            │
│  → Shard coordinator smart contract                    │
│  → Consistent hashing for user routing                 │
│  → Validator discovery and health                      │
└────────────┬────────────────────────────────────────────┘
             │
    ┌────────┴────────┬────────────────┬──────────────┐
    │                 │                │              │
    ▼                 ▼                ▼              ▼
┌─────────┐      ┌─────────┐     ┌─────────┐    ┌─────────┐
│Validator│      │Validator│     │Validator│    │Validator│
│   #1    │      │   #2    │     │   #3    │    │   ...   │
│ 100k    │      │ 100k    │     │ 100k    │    │         │
│ users   │      │ users   │     │ users   │    │         │
└────┬────┘      └────┬────┘     └────┬────┘    └────┬────┘
     │                │               │              │
     └────────────────┴───────────────┴──────────────┘
                          │
                          ▼
                ┌───────────────────┐
                │  S3 (Shared Tier) │
                │  → Cache sync     │
                │  → Snapshots      │
                └───────────────────┘
```

### Key Features

1. **Shard Coordinator Smart Contract**:
   - Maps user_id → validator using consistent hashing
   - Tracks validator health and capacity
   - Handles validator addition/removal

2. **Sticky Sessions**:
   - Same user always routes to same validator
   - Local cache remains hot for that user

3. **Cache Sync Protocol**:
   - Popular dependencies replicated across validators
   - S3 as shared tier for cache misses
   - Background sync during idle time

4. **Scaling Phases**:
   - **Phase 1 (0-100k)**: Single validator, local cache
   - **Phase 2 (100k-500k)**: 5 validators, regional S3
   - **Phase 3 (500k-1M)**: 10 validators, global S3 with CDN
   - **Phase 4 (1M+)**: Geographic distribution, hierarchical caching

## Storage Architecture

### Content-Addressed Blobs

All snapshots, checkpoints, and cached data stored as content-addressed blobs:

```
SHA256(data) → blob_id
/var/lib/faas/blobs/ab/cdef1234567890...
                    ^^  ^^^^^^^^^^^^^^^^^^
                    |   Rest of hash
                    First 2 chars (sharding)
```

### Tiered Storage

```
┌─────────────────────────────────────────────────────────┐
│ L1: Local NVMe (/var/lib/faas/blobs)                   │
│  → Hot cache, fastest access                            │
│  → 100GB-1TB typical                                    │
└────────────────────┬────────────────────────────────────┘
                     │ Cache miss
                     ▼
┌─────────────────────────────────────────────────────────┐
│ L2: S3 (Object Storage)                                 │
│  → Unlimited capacity                                   │
│  → Background replication                               │
│  → Automatic deduplication                              │
└─────────────────────────────────────────────────────────┘
```

**Configuration**:
```bash
# Local only (default)
cargo run --release --package faas-gateway-server

# Enable S3 tier (single env var)
export FAAS_OBJECT_STORE_URL=s3://my-bucket
cargo run --release --package faas-gateway-server
```

## Performance Characteristics

| Metric | Docker | Firecracker | Notes |
|--------|--------|-------------|-------|
| Cold start | 50-200ms | ~125ms | With pre-warming |
| Warm start | <10ms | <10ms | From cache |
| Isolation | Process | Hardware | Firecracker uses KVM |
| Best for | Development | Production | Auto-select with `Runtime::Auto` |

## Security Model

- **Multi-tenant isolation**: Each container runs in isolated environment
- **Resource quotas**: CPU, memory, disk limits per container
- **Network isolation**: Containers cannot access each other by default
- **Secure storage**: Content-addressed blobs prevent tampering
- **Firecracker**: Hardware-level isolation for production

## Extensibility

### Adding New Execution Modes

1. Add variant to `ExecutionMode` enum
2. Implement execution logic in platform executor
3. Update SDKs with new mode
4. Document use cases

### Adding New Runtimes

1. Implement `RuntimeExecutor` trait
2. Register in platform executor
3. Add runtime selection logic
4. Update SDK runtime options

### Adding New Event Types

1. Add variant to `StreamEvent` enum
2. Emit from container executor
3. Update SDK types
4. Document event format

## Implementation Status

✅ **Completed**:
- Dual runtime (Docker + Firecracker)
- 5 execution modes
- Content-addressed storage
- S3 integration
- Shared dependency caching
- WebSocket streaming API
- SDK support (TypeScript, Python, Rust)

🚧 **In Progress**:
- Container lifecycle management (create, stop, resume)
- Actual stdout/stderr streaming (currently stubbed)
- Stdin forwarding to containers

📋 **Planned**:
- Multi-node coordination (Tangle Blueprint)
- Advanced resource quotas
- Geographic distribution
- Monitoring and observability
- Auto-scaling policies

## References

- **Storage Quick Start**: `docs/QUICKSTART_STORAGE.md`
- **Storage Configuration**: `docs/STORAGE_CONFIGURATION.md`
- **Example - Basic Usage**: `examples/quickstart/`
- **Example - Advanced Features**: `examples/advanced-features/`
- **Example - WebSocket Streaming**: `examples/streaming-demo/`
- **Example - CI/CD**: `examples/cicd/`
- **Example - Data Pipeline**: `examples/data-pipeline/`
