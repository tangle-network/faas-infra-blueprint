#!/bin/bash
# Development environment setup for FaaS

set -e

OS=$(uname -s)

echo "🚀 Setting up FaaS development environment on $OS"

# Install Rust toolchain
if ! command -v rustc &> /dev/null; then
    echo "🦀 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

rustup default nightly
rustup component add rustfmt clippy

# Platform-specific setup
if [ "$OS" = "Darwin" ]; then
    echo "🍎 macOS detected - Setting up Docker Desktop"
    
    if ! command -v docker &> /dev/null; then
        echo "⚠️  Please install Docker Desktop from:"
        echo "https://www.docker.com/products/docker-desktop/"
        exit 1
    fi
    
    # Install development tools
    if command -v brew &> /dev/null; then
        brew install wget jq
    fi
    
    echo "ℹ️  Note: Firecracker and CRIU are not available on macOS."
    echo "Tests will use mock implementations."
    
elif [ "$OS" = "Linux" ]; then
    echo "🐧 Linux detected - Installing production dependencies"
    
    # Install Docker
    if ! command -v docker &> /dev/null; then
        curl -fsSL https://get.docker.com | sh
        sudo usermod -aG docker $USER
    fi
    
    # Install CRIU
    if ! command -v criu &> /dev/null; then
        if command -v apt-get &> /dev/null; then
            sudo apt-get update
            sudo apt-get install -y criu
        elif command -v yum &> /dev/null; then
            sudo yum install -y criu
        fi
    fi
    
    # Setup Firecracker
    echo "🚀 Run ./scripts/setup_firecracker.sh for Firecracker setup"
fi

# Install test coverage tools
cargo install cargo-tarpaulin || true
cargo install cargo-criterion || true

# Create required directories
mkdir -p var/lib/faas/{snapshots,artifacts,logs}
mkdir -p var/lib/faas/criu/{images,work}

# Run tests to verify setup
echo "🧪 Running test suite..."
cd crates/faas-executor

if [ "$OS" = "Darwin" ]; then
    echo "Running mock tests (macOS)..."
    cargo test --test mock_tests --test chaos_tests --test network_chaos
else
    echo "Running full test suite (Linux)..."
    cargo test --all-features
fi

echo "✅ Development environment ready!"
echo ""
echo "Quick commands:"
echo "  cargo test                    # Run all tests"
echo "  cargo bench                   # Run benchmarks"
echo "  cargo tarpaulin               # Generate coverage report"
echo "  cargo test --test docker_integration -- --ignored  # Run real Docker tests"