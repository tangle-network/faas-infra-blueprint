# FaaS Platform Examples

Complete collection of examples demonstrating FaaS capabilities across multiple SDKs and use cases.

## Quick Start

```bash
# 1. Start the FaaS gateway server
cargo run --release --package faas-gateway-server

# 2. Run any example
cargo run --release --package quickstart
```

## Rust SDK Examples

All Rust examples use the `faas-sdk` crate and build successfully.

| Example | Lines | Features | Use Case |
|---------|-------|----------|----------|
| [quickstart](./quickstart/) | 44 | Basic execution | Getting started |
| [advanced-features](./advanced-features/) | 112 | Forking, snapshots, multi-lang | Complete SDK tour |
| [cicd](./cicd/) | 357 | Parallel testing, security | CI/CD pipelines |
| [data-pipeline](./data-pipeline/) | 292 | ETL workflow | Data processing |
| [opencode-cloud-dev](./opencode-cloud-dev/) | 173 | OpenCode integration | AI code generation |
| [zk-faas](./zk-faas/) | 295 | ZK proofs | Privacy computation |

## Python SDK Examples

Located in `python/`:

- **quickstart.py** (116 lines): Basic operations, caching, metrics
- **advanced.py** (305 lines): Forking, ML workflows, data pipelines, Firecracker security, streaming logs

```bash
cd examples/python
python3 quickstart.py
python3 advanced.py
```

## TypeScript SDK Examples

Located in `typescript/`:

- **quickstart.ts** (171 lines): Multi-language execution, caching, events, method chaining

```bash
cd examples/typescript
npm install
npm start
```

## Feature Coverage Matrix

| Feature | Rust | Python | TypeScript |
|---------|:----:|:------:|:----------:|
| Basic execution | ✅ | ✅ | ✅ |
| Multi-language helpers | ✅ | ✅ | ✅ |
| Runtime selection | ✅ | ✅ | ✅ |
| Client-side caching | ✅ | ✅ | ✅ |
| Pre-warming | ✅ | ✅ | ✅ |
| Forking/Branching | ✅ | ✅ | ✅ |
| Snapshots | ✅ | ✅ | ✅ |
| Event emitters | ✅ | ✅ | ✅ |
| Log streaming | ✅ | ✅ | ✅ |
| Metrics | ✅ | ✅ | ✅ |
| ML workflows | ❌ | ✅ | ❌ |
| Data pipelines | ✅ | ✅ | ❌ |
| CI/CD | ✅ | ❌ | ❌ |
| ZK proofs | ✅ | ❌ | ❌ |

## Learning Path

1. **Start Here**: `quickstart/` - Basic SDK usage
2. **Core Features**: `advanced-features/` - Forking, snapshots, caching
3. **Real Workflows**: Choose based on your use case:
   - CI/CD: `cicd/`
   - Data: `data-pipeline/`
   - AI: `opencode-cloud-dev/`
   - Privacy: `zk-faas/`

## SDK-Specific Examples

### For Python Developers
```bash
cd examples/python
python3 quickstart.py    # Basic features
python3 advanced.py      # ML, pipelines, security
```

### For TypeScript Developers
```bash
cd examples/typescript
npm install
ts-node quickstart.ts    # Comprehensive demo
```

### For Rust Developers
```bash
cargo run --release --package quickstart          # Start here
cargo run --release --package advanced-features   # Deep dive
cargo run --release --package cicd                # Production workflow
```

## Building All Examples

```bash
# Test all Rust examples build
cargo build --release \
  --package quickstart \
  --package advanced-features \
  --package cicd \
  --package data-pipeline \
  --package opencode-cloud-dev \
  --package zk-faas-example
```

## Architecture

All examples follow this pattern:

1. **Create client**: Connect to FaaS gateway (default: http://localhost:8080)
2. **Execute commands**: Use SDK methods (run, execute, fork, etc.)
3. **Handle results**: Process stdout, stderr, metrics
4. **Cleanup**: Automatic resource management

## Advanced Use Cases

### Forking for A/B Testing
```rust
let base = client.execute(base_request).await?;
let variant_a = client.fork_execution(&base.request_id, "variant A").await?;
let variant_b = client.fork_execution(&base.request_id, "variant B").await?;
```

### Snapshot-Based Development
```rust
// Create snapshot once
let snapshot = client.create_snapshot(dev_env_config).await?;

// Reuse instantly
let result = client.execute_from_snapshot(snapshot.id).await?;
```

### Multi-Language Pipeline
```rust
let python_result = client.run_python("import pandas as pd").await?;
let js_result = client.run_javascript("console.log('Processing')").await?;
let bash_result = client.run_bash("./deploy.sh").await?;
```

## SDK Documentation

- **Rust SDK**: `crates/faas-sdk/src/lib.rs`
- **Python SDK**: `sdks/python/faas_sdk.py`
- **TypeScript SDK**: `sdks/typescript/src/index.ts`

## Troubleshooting

### Server Not Running
```bash
# Start the gateway first
cargo run --release --package faas-gateway-server
```

### Docker Permission Errors
```bash
# Add user to docker group
sudo usermod -aG docker $USER
newgrp docker
```

### Port Already in Use
```bash
# Kill existing server
pkill -f faas-gateway-server

# Or use different port
FAAS_PORT=8081 cargo run --release --package faas-gateway-server
```

## Contributing

When adding new examples:

1. ✅ Ensure example builds successfully
2. ✅ Add README.md with features and usage
3. ✅ Add to workspace Cargo.toml
4. ✅ Update this README with the new example
5. ✅ Test end-to-end functionality

## License

MIT OR Apache-2.0
