# FaaS Platform Examples

Production-ready services and comprehensive examples demonstrating the full capabilities of the FaaS platform.

## 🚀 Quick Start

```bash
# Start the FaaS gateway server
cargo run --package faas-gateway-server &

# Run specific examples
cargo run --package quickstart
cargo run --package gpu-service
cargo run --package distributed-agents
cargo run --package opencode-cloud-dev
```

## 📚 All Examples

### Core Functionality
- **[quickstart](./quickstart)** - Simple getting started example with basic function execution
- **[advanced-features](./advanced-features)** - Advanced platform features (caching, persistence, optimization)
- **[api-showcase](./api-showcase)** - Complete demonstration of all API endpoints
- **[comprehensive-demo](./comprehensive-demo)** - Full-stack application with all features

### Development Environments
- **[opencode-cloud-dev](./opencode-cloud-dev)** - Cloud-based AI agent development with OpenCode Server
- **[remote-dev](./remote-dev)** - Remote development with code-server, Jupyter, desktop environments
- **[remote_desktop.rs](./remote_desktop.rs)** - Virtual desktop environment in containers

### AI & Machine Learning
- **[ai_sandbox.rs](./ai_sandbox.rs)** - Sandboxed AI model execution environment
- **[gpu-service](./gpu-service)** - GPU-accelerated ML model serving (<100ms warm starts)
- **[distributed-agents](./distributed-agents)** - Multi-agent systems with branching (50x speedup)

### SDK Examples
- **[sdk-integration](./sdk-integration)** - Rust SDK integration patterns
- **[python](./python)** - Python SDK quickstart and advanced examples
- **[typescript](./typescript)** - TypeScript SDK examples
- **[rust](./rust)** - Advanced Rust SDK patterns

### Infrastructure & DevOps
- **[real-cicd](./real-cicd)** - CI/CD pipeline integration
- **[real-data-pipeline](./real-data-pipeline)** - Data processing with streaming and batch
- **[browser_automation.rs](./browser_automation.rs)** - Headless browser automation

### Specialized Use Cases
- **[zk-faas](./zk-faas)** - Zero-knowledge proof generation and verification

## 💻 Running Examples

### Basic Execution
```bash
# Quickstart example
cargo run --package quickstart

# With custom configuration
FAAS_ENDPOINT=http://localhost:8080 cargo run --package advanced-features
```

### Python Examples
```bash
# Install dependencies
cd sdks/python
pip install -r requirements.txt

# Run examples
python examples/python/quickstart.py
python examples/python/advanced.py
```

### TypeScript Examples
```bash
# Install dependencies
cd sdks/typescript
npm install

# Run examples
npm run example:quickstart
```

## 🏆 Featured Examples

### 1. **OpenCode Cloud Development**
Complete cloud IDE with AI agent support
```bash
cargo run --package opencode-cloud-dev
# Access at: http://localhost:4096
```

### 2. **GPU Service**
ML model serving with GPU acceleration
```bash
cargo run --package gpu-service
# 30s cold start → <100ms warm (300x speedup)
```

### 3. **Distributed Agents**
Multi-agent parallel exploration
```bash
cargo run --package distributed-agents
# 550s setup → 10s restore (50x speedup)
```

### 4. **Remote Development**
Full development environment in the cloud
```bash
cargo run --package remote-dev
# Jupyter: http://localhost:8888
# VSCode: http://localhost:3000
# Desktop: http://localhost:6080
```

## 📊 Performance Benchmarks

| Example | Cold Start | Warm Start | Speedup |
|---------|------------|------------|---------|
| GPU Service | 30s | <100ms | 300x |
| Agent Branching | 550s | 10s | 50x |
| Quickstart | 500ms | 24μs | 20,000x |
| Data Pipeline | 2s | 50ms | 40x |

## 🔧 Requirements

- Docker or Podman
- Rust nightly (`rustup install nightly`)
- Optional: CUDA toolkit for GPU examples
- Optional: Node.js 18+ for TypeScript examples
- Optional: Python 3.9+ for Python examples

## 🧪 Testing

```bash
# Run all example tests
./examples/test_examples.sh

# Test specific example
cargo test --package quickstart

# Run with real Docker (no mocks)
cargo test --package gpu-service -- --ignored
```

## 📖 Documentation

Each example includes:
- Comprehensive README with usage instructions
- Inline code documentation
- Performance benchmarks
- Security considerations
- Scaling guidelines

## 🤝 Contributing

To add a new example:
1. Create directory under `examples/`
2. Add `Cargo.toml` with dependencies
3. Implement in `src/main.rs`
4. Add comprehensive `README.md`
5. Update this file
6. Add tests
7. Submit PR

## 🔐 Security Notes

- All examples run in isolated containers
- Resource limits enforced by default
- Network policies configurable
- Secrets managed via environment variables
- Audit logging available