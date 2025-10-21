# FaaS Platform

High-performance serverless execution platform with Docker containers and Firecracker microVMs. Supports both HTTP gateway and Tangle blockchain integration for decentralized operation.

## Architecture

Two deployment modes with shared execution core:

```
┌────────────────────────────────────────────────────────────┐
│                    HTTP Gateway Mode                       │
│  REST API → Gateway Server → Platform Executor            │
│  Use case: Development, testing, single-node deployment   │
└────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│                 Tangle Blockchain Mode                     │
│  Smart Contract → Multiple Operators → Platform Executor  │
│  Use case: Production, multi-operator, decentralized      │
└────────────────────────────────────────────────────────────┘

             Both modes use: Platform Executor
                     ↓
          ┌──────────┴──────────┐
          │                     │
    Docker Runtime      Firecracker Runtime
```

## Quick Start

### HTTP Gateway Mode

```bash
# Start gateway server
cargo run --release --package faas-gateway-server

# Use any SDK
npm install @faas-platform/sdk  # TypeScript
pip install faas-sdk            # Python
# Rust: add faas-sdk to Cargo.toml
```

### Tangle Blockchain Mode

```bash
# Deploy blueprint to Tangle
cd faas-bin && cargo build --release

# Run as operator (registers automatically)
cargo run --release --bin faas-blueprint

# Use blockchain SDK (optional feature)
cargo add faas-sdk --features tangle
```

## SDK Examples

### Rust (HTTP Gateway)

```rust
use faas_sdk::{FaasClient, ExecuteRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = FaasClient::new("http://localhost:8080".to_string());

    // Execute command
    let result = client.execute(ExecuteRequest {
        command: "echo 'Hello, World!'".to_string(),
        image: Some("alpine:latest".to_string()),
        timeout_ms: Some(5000),
        ..Default::default()
    }).await?;

    println!("Output: {}", result.stdout);
    println!("Duration: {}ms", result.duration_ms);
    Ok(())
}
```

### Rust (Tangle Blockchain)

```rust
use faas_sdk::tangle::TangleClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable with: cargo add faas-sdk --features tangle
    let client = TangleClient::new("ws://localhost:9944").await?;

    // Submit job to blockchain (executed by operators)
    let result = client.execute_function(
        "alpine:latest",
        vec!["echo", "Hello from blockchain!"],
        None,
        vec![]
    ).await?;

    println!("Job call ID: {}", result.call_id);
    println!("Output: {:?}", result.result);
    Ok(())
}
```

### TypeScript (HTTP Gateway)

```typescript
import { FaaSClient, Runtime } from '@faas-platform/sdk';

const client = new FaaSClient('http://localhost:8080');

// Execute JavaScript
const result = await client.runJavaScript('console.log("Hello!")');
console.log(result.output);

// Execute Python
const pythonResult = await client.runPython('print("Hello")');
console.log(pythonResult.output);

// Advanced execution
const advanced = await client.execute({
  command: 'npm test',
  image: 'node:20-slim',
  runtime: Runtime.Docker,
  envVars: { NODE_ENV: 'test' },
  timeoutMs: 30000
});
```

### Python (HTTP Gateway)

```python
from faas_sdk import FaaSClient

client = FaaSClient("http://localhost:8080")

# Execute Python code
result = await client.run_python('print("Hello")')
print(result.output)

# Execute with caching
cached_result = await client.execute(
    command='pytest tests/',
    image='python:3.11-slim',
    mode='cached',
    timeout_ms=60000
)

# Fork execution for A/B testing
base = await client.execute(command='setup.sh')
variant_a = await client.fork_execution(base.request_id, 'test_a.py')
variant_b = await client.fork_execution(base.request_id, 'test_b.py')
```

## Complete Feature Matrix

### HTTP Gateway Features

| Feature | Rust SDK | TypeScript SDK | Python SDK |
|---------|:--------:|:--------------:|:----------:|
| Basic execution | ✓ | ✓ | ✓ |
| Multi-language helpers | ✓ | ✓ | ✓ |
| Runtime selection | ✓ | ✓ | ✓ |
| Client-side caching | ✓ | ✓ | ✓ |
| Snapshots | ✓ | ✓ | Partial |
| Forking/branching | ✓ | ✓ | ✓ |
| Pre-warming | ✓ | ✓ | ✓ |
| Event emitters | ✓ | ✓ | ✓ |
| Metrics | ✓ | ✓ | ✓ |

### Tangle Blockchain Features

| Feature | Rust SDK | TypeScript SDK |
|---------|:--------:|:--------------:|
| Submit jobs | ✓ (feature-gated) | Structure ready |
| Query results | ✓ (feature-gated) | Structure ready |
| Operator assignment | ✓ (feature-gated) | Structure ready |
| 12 job types | ✓ (defined) | ✓ (defined) |

## Execution Modes

### Ephemeral (Default)
Container destroyed after execution. Fastest for stateless operations.

```rust
client.execute(ExecuteRequest {
    command: "echo test".to_string(),
    mode: Some("ephemeral".to_string()),
    ..Default::default()
}).await?
```

### Cached
Results cached by content hash. Subsequent identical requests return instantly.

```rust
client.execute(ExecuteRequest {
    command: "expensive_computation".to_string(),
    mode: Some("cached".to_string()),
    cache_key: Some("computation-v1".to_string()),
    ..Default::default()
}).await?
```

### Checkpointed
CRIU-based checkpointing. Save and restore execution state.

```rust
// Create checkpoint
let result = client.execute(ExecuteRequest {
    command: "train_model.py".to_string(),
    mode: Some("checkpointed".to_string()),
    ..Default::default()
}).await?;

// Resume from checkpoint
client.execute(ExecuteRequest {
    command: "continue_training.py".to_string(),
    mode: Some("checkpointed".to_string()),
    snapshot_id: Some(result.request_id),
    ..Default::default()
}).await?
```

### Branched
Fork execution for A/B testing and parallel paths.

```rust
let base = client.execute(ExecuteRequest {
    command: "setup_env.sh".to_string(),
    ..Default::default()
}).await?;

let variant_a = client.fork_execution(&base.request_id, "algo_v1.py").await?;
let variant_b = client.fork_execution(&base.request_id, "algo_v2.py").await?;
```

### Persistent
Long-running containers with manual lifecycle control.

```rust
let instance = client.create_instance(CreateInstanceRequest {
    name: Some("dev-env".to_string()),
    image: "ubuntu:22.04".to_string(),
    persistent: Some(true),
    ..Default::default()
}).await?;

// Container stays alive, execute multiple commands
```

## Tangle Blockchain Integration

The platform can be deployed as a Tangle blueprint for decentralized multi-operator execution.

### Job Types (12 Total)

| Job ID | Name | Description |
|--------|------|-------------|
| 0 | Execute Function | Basic container execution |
| 1 | Execute Advanced | Execution with modes (cached, checkpointed, branched, persistent) |
| 2 | Create Snapshot | CRIU checkpoint creation |
| 3 | Restore Snapshot | Restore from checkpoint |
| 4 | Create Branch | Fork execution path |
| 5 | Merge Branches | Combine execution results |
| 6 | Start Instance | Launch persistent container |
| 7 | Stop Instance | Terminate instance |
| 8 | Pause Instance | Suspend with checkpoint |
| 9 | Resume Instance | Resume from pause |
| 10 | Expose Port | Network port mapping |
| 11 | Upload Files | File transfer to container |

### Smart Contract

Located at `contracts/src/FaaSBlueprint.sol`:
- Operator registration and load balancing
- Job assignment tracking
- Result validation and storage
- Event emission for observability

### Running as Operator

```bash
# Build operator binary
cd faas-bin && cargo build --release

# Operator automatically:
# 1. Registers with Tangle
# 2. Listens for job assignments
# 3. Executes via platform executor
# 4. Submits results to blockchain
cargo run --release --bin faas-blueprint
```

### Submitting Jobs (User-side)

```rust
use faas_sdk::tangle::TangleClient;

let client = TangleClient::new("ws://localhost:9944").await?;

// Job gets assigned to operator via load balancing
let result = client.execute_function(
    "alpine:latest",
    vec!["echo", "test"],
    None,
    vec![]
).await?;

// Query which operator executed it
let operator = client.get_assigned_operator(result.call_id).await?;
```

## Runtime Selection

| Runtime | Cold Start | Security | Platform Support |
|---------|------------|----------|------------------|
| Docker | 50-200ms | Process isolation | All platforms |
| Firecracker | ~125ms | Hardware isolation | Linux only |
| Auto | Varies | Adaptive selection | All platforms |

### Docker Runtime
```rust
let client = FaasClient::with_runtime(
    "http://localhost:8080".to_string(),
    Runtime::Docker
);
```

### Firecracker Runtime
```rust
let client = FaasClient::with_runtime(
    "http://localhost:8080".to_string(),
    Runtime::Firecracker
);
```

## Storage Configuration

Local storage (default, no configuration):
```bash
cargo run --release --package faas-gateway-server
```

S3-compatible storage (single environment variable):
```bash
export FAAS_OBJECT_STORE_URL=s3://my-bucket
cargo run --release --package faas-gateway-server
```

Supports: AWS S3, MinIO, Cloudflare R2, DigitalOcean Spaces

## API Endpoints (Gateway Mode)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/execute` | POST | Execute command |
| `/api/v1/fork` | POST | Fork execution |
| `/api/v1/snapshots` | POST | Create snapshot |
| `/api/v1/snapshots` | GET | List snapshots |
| `/api/v1/instances` | POST | Create instance |
| `/api/v1/instances` | GET | List instances |
| `/api/v1/metrics` | GET | Performance metrics |
| `/health` | GET | Health check |
| `/api/v1/containers/:id/stream` | WebSocket | Bidirectional streaming |

## Examples

Complete working examples in `examples/`:

| Example | Language | Features | Lines |
|---------|----------|----------|-------|
| quickstart | Rust | Basic execution | 44 |
| advanced-features | Rust | Forking, snapshots, multi-lang | 112 |
| cicd | Rust | Parallel testing, security | 357 |
| data-pipeline | Rust | ETL workflow | 292 |
| python | Python | ML workflows, streaming | 305 |
| typescript | TypeScript | Multi-language, events | 171 |

Run any example:
```bash
cargo run --release --package quickstart
cargo run --release --package advanced-features
cd examples/python && python3 advanced.py
cd examples/typescript && npm start
```

## Building

```bash
# Build gateway server
cargo build --release --package faas-gateway-server

# Build operator (Tangle mode)
cargo build --release --bin faas-blueprint

# Build all examples
cargo build --release --workspace --exclude faas-zk-prover

# Run tests
cargo test --workspace
```

## Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `FAAS_OBJECT_STORE_URL` | S3 storage URL | None (local only) |
| `AWS_ACCESS_KEY_ID` | AWS credentials | - |
| `AWS_SECRET_ACCESS_KEY` | AWS credentials | - |
| `AWS_REGION` | AWS region | us-east-1 |
| `AWS_ENDPOINT` | Custom S3 endpoint | - |

## Requirements

- Rust: 1.70+ (nightly for operator mode)
- Docker: For container runtime
- Node.js: 16+ for TypeScript SDK
- Python: 3.8+ for Python SDK
- Linux: For Firecracker support (optional)

## Documentation

- Complete feature documentation: `ARCHITECTURE.md`
- Tangle integration details: `COMPREHENSIVE_FAAS_BLUEPRINT.md`
- Operator selection design: `OPERATOR_SELECTION_DESIGN.md`
- Storage configuration: `docs/QUICKSTART_STORAGE.md`
- SDK documentation:
  - Rust: `crates/faas-sdk/src/lib.rs`
  - TypeScript: `sdks/typescript/README.md`
  - Python: `sdks/python/README.md`
  - Tangle (Rust): `crates/faas-sdk/src/tangle.rs` (feature-gated)
  - Tangle (TS): `sdks/typescript-tangle/README.md` (structure)

## License

MIT OR Apache-2.0
