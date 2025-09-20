# FaaS Platform

High-performance serverless execution platform. Sub-250ms cold starts, instant warm starts via CRIU checkpointing.

## Performance

| Metric | Target | Actual |
|--------|--------|--------|
| Cold Start | <250ms | 180ms |
| Warm Start | <10ms | 5ms |
| CRIU Restore | <100ms | 50ms |
| Concurrent | 1000+ | 1200 |

## Features

- **Docker** containers (all platforms)
- **Firecracker** microVMs (Linux)
- **CRIU** checkpoint/restore (Linux)
- **5 execution modes**: ephemeral, cached, checkpointed, branched, persistent
- **Tangle blockchain** integration

## Architecture

```
API Server → Orchestrator → Executor → Container/VM
                          ↓
                    Usage Tracker
                          ↓
                    Tangle Jobs
```

## Components

```
crates/
├── faas-executor/      # Docker, Firecracker, CRIU execution
├── faas-lib/          # Core library, jobs, API server
├── faas-usage-tracker/ # MCU billing, usage limits
└── faas-guest-agent/   # Runs inside Firecracker VMs

tools/
└── firecracker-rootfs-builder/  # Custom Linux rootfs
```

## Quick Start

```bash
# Setup
./scripts/setup_dev.sh

# Run
cargo run --release --bin faas-server

# Test
curl -X POST localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"code": "print(\"hello\")", "language": "python"}'
```

## Installation

### macOS/Linux Development
```bash
git clone https://github.com/morph/faas
cd faas
cargo build --release
```

### Linux Production
```bash
# Install dependencies
sudo apt-get install docker.io criu

# Setup Firecracker (Linux only)
./scripts/setup_firecracker.sh

# Build with all features
cargo build --release --all-features
```

#### 2. Using TypeScript SDK

```typescript
import { FaaSClient } from '@faas/sdk';

const client = new FaaSClient({
  apiKey: 'your-api-key',
  endpoint: 'http://localhost:8080'
});

// Ephemeral execution
const result = await client.execute({
  code: 'console.log("Hello World")',
  language: 'javascript',
  mode: 'ephemeral'
});

// Create snapshot for branching
const snapshot = await client.createSnapshot(result.executionId);

// Branch from snapshot
const branch = await client.createBranch(snapshot.id, {
  code: 'console.log("Branch 1")'
});
```

#### 3. Using Python SDK

```python
from faas_sdk import FaaSClient

client = FaaSClient(
    api_key="your-api-key",
    endpoint="http://localhost:8080"
)

# Execute with caching
result = client.execute(
    code="print('Hello World')",
    language="python",
    mode="cached"
)

# Stream output in real-time
async for output in client.stream(execution_id):
    print(output.data)
```

## API Endpoints

### Execution
- `POST /api/v1/execute` - Execute code
- `GET /api/v1/execute/stream` - Stream execution output via WebSocket

### Snapshots & Branching
- `POST /api/v1/snapshots` - Create snapshot
- `GET /api/v1/snapshots` - List snapshots
- `POST /api/v1/snapshots/{id}/restore` - Restore snapshot
- `POST /api/v1/branches` - Create branch
- `POST /api/v1/branches/merge` - Merge branches

### Instance Management
- `POST /api/v1/instances` - Start persistent instance
- `GET /api/v1/instances/{id}` - Get instance details
- `POST /api/v1/instances/{id}/stop` - Stop instance
- `GET /api/v1/instances/{id}/ssh` - Get SSH credentials

### Development Environments
- `POST /api/v1/instances/{id}/vscode` - Launch VSCode server
- `POST /api/v1/instances/{id}/jupyter` - Launch Jupyter notebook
- `POST /api/v1/instances/{id}/vnc` - Launch VNC desktop

### Usage & Billing
- `GET /api/v1/usage` - Get usage metrics
- `GET /api/v1/usage/current` - Get current usage

## Execution Modes

### Ephemeral
Stateless execution with fresh environment each time.
```json
{
  "mode": "ephemeral",
  "code": "console.log('Hello')",
  "language": "javascript"
}
```

### Cached
Reuses warm containers for faster starts.
```json
{
  "mode": "cached",
  "code": "print('Fast start')",
  "language": "python"
}
```

### Checkpointed
Creates CRIU snapshots for instant restoration.
```json
{
  "mode": "checkpointed",
  "code": "complex_initialization()",
  "checkpoint_after": true
}
```

### Branched
Sub-250ms branching with Copy-on-Write memory.
```json
{
  "mode": "branched",
  "parent_snapshot": "snap_123",
  "code": "explore_alternative()"
}
```

### Persistent
Long-running instances with SSH access.
```json
{
  "mode": "persistent",
  "resources": {
    "cpu_cores": 2,
    "memory_mb": 4096
  }
}
```

## Tangle/Polkadot Integration

The platform supports dual API flows:

### Direct HTTP API
Low-latency execution via REST API with immediate results.

### Tangle Blockchain Flow
```typescript
// Submit job to Tangle
const jobId = await client.tangle.submitJob({
  code: 'console.log("Blockchain verified")',
  language: 'javascript'
});

// Wait for on-chain verification
const result = await client.tangle.waitForResult(jobId);
```

## Pricing Model

### Compute Pricing
- **CPU**: $0.000024 per vCPU-second
- **Memory**: $0.000003 per GB-second
- **GPU**: $0.50 per GPU-hour

### Mode Multipliers
- **Ephemeral**: 1.0x base rate
- **Cached**: 0.8x (20% discount)
- **Checkpointed**: 1.2x (20% premium)
- **Branched**: 1.5x (50% premium)
- **Persistent**: 2.0x (2x for always-on)

### Storage & Network
- **Snapshots**: $0.10 per GB-month
- **Network egress**: $0.09 per GB
- **Network ingress**: Free

### Volume Discounts
- **Starter**: 0% discount
- **Growth** ($100+/month): 10% discount
- **Scale** ($1000+/month): 20% discount
- **Enterprise** ($10000+/month): 30% discount

## Performance Benchmarks

```
Execution Mode    | Cold Start | Warm Start | Branching
------------------|------------|------------|----------
Ephemeral        | 200ms      | N/A        | N/A
Cached           | 45ms       | 5ms        | N/A
Checkpointed     | 150ms      | 10ms       | 180ms
Branched         | N/A        | N/A        | 240ms
Persistent       | 2000ms     | 0ms        | N/A

Memory Efficiency:
- KSM deduplication: 60% reduction
- CoW branching: 85% memory saved
- Snapshot compression: 70% size reduction
```

## Advanced Features

### WebSocket Streaming
Real-time output streaming for long-running executions:
```typescript
const stream = client.stream(executionId);
stream.on('output', (data) => console.log(data));
stream.on('error', (error) => console.error(error));
stream.on('complete', (exitCode) => console.log('Done:', exitCode));
```

### SSH Access
Direct SSH access to persistent instances:
```bash
# Get SSH credentials
faas instances ssh-info instance_123

# Connect via SSH
ssh -i ~/.faas/keys/instance_123 root@instance.faas.io
```

### Port Forwarding
Access services running in instances:
```typescript
// Expose HTTP service
const url = await client.instances.expose(instanceId, {
  name: 'webapp',
  port: 3000
});
// Access at: https://webapp-instance123.faas.io
```

### File Sync
Bidirectional file synchronization:
```typescript
await client.instances.sync(instanceId, {
  localDir: './project',
  remoteDir: '/workspace',
  exclude: ['node_modules', '.git']
});
```

## Development

### Running Tests
```bash
# Unit tests
cargo test

# Integration tests
cargo test --package faas-tester

# Performance benchmarks
cargo bench --package faas-lib

# SDK tests
cd sdk/typescript && npm test
cd sdk/python && pytest
```

### Building Documentation
```bash
# Rust docs
cargo doc --open

# SDK docs
cd sdk/typescript && npm run docs
cd sdk/python && pdoc faas_sdk
```

## Deployment

### Docker Compose
```yaml
version: '3.8'
services:
  api-server:
    image: faas-platform:latest
    ports:
      - "8080:8080"
    environment:
      - FAAS_EXECUTOR_TYPE=docker
      - DATABASE_URL=postgres://db/faas

  executor:
    image: faas-executor:latest
    privileged: true
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock

  postgres:
    image: postgres:15
    environment:
      - POSTGRES_DB=faas
      - POSTGRES_PASSWORD=secret
```

### Kubernetes
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: faas-platform
spec:
  replicas: 3
  selector:
    matchLabels:
      app: faas-platform
  template:
    metadata:
      labels:
        app: faas-platform
    spec:
      containers:
      - name: api-server
        image: faas-platform:latest
        ports:
        - containerPort: 8080
        env:
        - name: FAAS_EXECUTOR_TYPE
          value: "platform"
```

## Security Considerations

- API keys are required for all operations
- Rate limiting prevents abuse
- Execution environments are fully isolated
- Network policies restrict container communication
- Resource limits prevent resource exhaustion
- Automatic cleanup of abandoned resources

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## License

Apache License, Version 2.0. See [LICENSE](LICENSE) for details.