> [!WARNING]
> **Deprecated.** This crate is part of `faas-infra-blueprint` which is in maintenance-only mode. New SDK work targets product blueprint SDK repos. See [MIGRATION.md](../../MIGRATION.md).

# FaaS Platform Rust SDK

[![Crate](https://img.shields.io/crates/v/faas-sdk.svg)](https://crates.io/crates/faas-sdk)
[![Documentation](https://docs.rs/faas-sdk/badge.svg)](https://docs.rs/faas-sdk)

High-performance Rust SDK for the FaaS Platform, providing serverless execution with both Docker containers and Firecracker microVMs.

## Features

- 🚀 **Dual Runtime Support**: Docker containers and Firecracker VMs
- 📊 **Intelligent Caching**: Multi-level result caching
- 🔥 **Pre-warming**: Zero cold starts with warm pools
- 🌳 **Execution Forking**: A/B testing and parallel workflows
- 📈 **Auto-scaling**: Predictive scaling based on load patterns
- 📋 **Rich Metrics**: Built-in performance monitoring
- 🔒 **Type Safety**: Full Rust type safety and error handling

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
faas-sdk = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

Basic usage:

```rust
use faas_sdk::{FaasClient, ExecuteRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = FaasClient::new("http://localhost:8080".to_string());

    let result = client.execute(ExecuteRequest {
        command: "echo 'Hello from Rust!'".to_string(),
        image: Some("alpine:latest".to_string()),
        env_vars: None,
        working_dir: None,
        timeout_ms: Some(5000),
    }).await?;

    println!("Output: {}", result.stdout);
    Ok(())
}
```

## Documentation

For detailed documentation, run:

```bash
cargo doc --open
```

## Examples

See the [examples directory](../../examples/rust/) for complete examples including:

- Basic execution
- Runtime selection
- Advanced workflows
- Performance optimization

## Performance

| Runtime | Cold Start | Security | Use Case |
|---------|------------|----------|----------|
| Docker | 50-200ms | Process isolation | Development, testing |
| Firecracker | ~125ms | Hardware isolation | Production, multi-tenant |
| Auto | Varies | Adaptive | Automatic selection |

## License

This project is licensed under the MIT License.