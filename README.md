# FaaS Platform

High-performance serverless execution with Docker containers and Firecracker microVMs.

## Quick Start

```bash
# 1. Start the server
cargo build --release
./target/release/faas-gateway-server

# 2. Use any SDK
npm install @faas-platform/sdk  # TypeScript/JavaScript
pip install faas-sdk            # Python
# Rust SDK in crates/faas-sdk
```

### TypeScript Example

```typescript
import { FaaSClient } from '@faas-platform/sdk';

const client = new FaaSClient('http://localhost:8080');
const result = await client.runJavaScript('console.log("Hello!")');
console.log(result.output); // Hello!
```

### Python Example

```python
from faas_sdk import FaaSClient

client = FaaSClient("http://localhost:8080")
result = await client.run_python('print("Hello!")')
print(result.output)  # Hello!
```

### Rust Example

```rust
use faas_sdk::FaasClient;

let client = FaasClient::new("http://localhost:8080");
let result = client.execute()
    .command("echo 'Hello!'")
    .send()
    .await?;
println!("{}", result.output);
```

## Features

- **Dual Runtime**: Docker (development) + Firecracker (production)
- **Smart Caching**: Automatic result caching with deduplication
- **Pre-warming**: Zero cold starts with warm container pools
- **Execution Forking**: Branch workflows for A/B testing
- **Checkpointing**: Save/restore execution state
- **S3 Storage**: Optional cloud storage (one env var)
- **Multi-language**: Python, JavaScript/TypeScript, Rust, Bash

## Storage Configuration

**Local (default):** Works automatically, no config needed.

**S3 (optional):** Add one environment variable:

```bash
export FAAS_OBJECT_STORE_URL=s3://my-bucket
./target/release/faas-gateway-server
```

See [Storage Quick Start](./docs/QUICKSTART_STORAGE.md) for AWS S3, MinIO, R2, DO Spaces examples.

## Performance

| Runtime | Cold Start | Security | Best For |
|---------|------------|----------|----------|
| Docker | 50-200ms | Process | Development, testing |
| Firecracker | ~125ms | Hardware | Production, multi-tenant |

## SDK Documentation

- **TypeScript**: [`sdks/typescript/README.md`](./sdks/typescript/README.md)
- **Python**: [`sdks/python/README.md`](./sdks/python/README.md)
- **Rust**: [`crates/faas-sdk/src/lib.rs`](./crates/faas-sdk/src/lib.rs)

## Examples

Complete working examples in [`examples/`](./examples/):

- **quickstart** - Basic execution (44 lines)
- **advanced-features** - Forking, snapshots, multi-language (112 lines)
- **cicd** - Parallel testing, security scanning (357 lines)
- **data-pipeline** - ETL workflows (292 lines)

Run any example:

```bash
cargo run --release --package quickstart
```

## Gateway Server API

The gateway server exposes these endpoints:

- `POST /api/v1/execute` - Execute commands
- `POST /api/v1/fork` - Fork execution for A/B testing
- `POST /api/v1/prewarm` - Pre-warm containers
- `POST /api/v1/snapshots` - Create snapshots
- `GET /api/v1/metrics` - Performance metrics
- `GET /health` - Health check

## Architecture

```
┌─────────────────────────────────────────┐
│  SDK (TypeScript/Python/Rust)          │
│  → Simple API, zero config             │
└─────────────┬───────────────────────────┘
              │ HTTP
              ▼
┌─────────────────────────────────────────┐
│  FaaS Gateway Server                    │
│  → Dual runtime support                 │
│  → Automatic optimization               │
└─────────────┬───────────────────────────┘
              │
      ┌───────┴────────┐
      │                │
      ▼                ▼
┌─────────────┐  ┌──────────────┐
│   Docker    │  │ Firecracker  │
│ Development │  │  Production  │
│  50-200ms   │  │    ~125ms    │
└─────────────┘  └──────────────┘
```

## Building

```bash
# Build gateway server
cargo build --release --package faas-gateway-server

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

- **Rust**: 1.70+
- **Docker**: For container runtime
- **Linux**: For Firecracker support (optional)
- **Node.js**: 16+ for TypeScript SDK
- **Python**: 3.8+ for Python SDK

## License

MIT OR Apache-2.0
