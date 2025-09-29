#!/bin/bash

# Start the FaaS Gateway Server

echo "🚀 Starting FaaS Gateway Server..."

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "❌ Docker is not running. Please start Docker first."
    exit 1
fi

echo "✅ Docker is running"

# Build the gateway
echo "🔨 Building gateway server..."
cargo build --package faas-gateway-server --release

# Run the gateway
echo "🌐 Starting gateway on http://localhost:8080"
echo ""
echo "Available endpoints:"
echo "  POST   /api/v1/execute          - Execute a function"
echo "  POST   /api/v1/execute/advanced - Advanced execution with modes"
echo "  POST   /api/v1/snapshots         - Create snapshot"
echo "  GET    /api/v1/snapshots         - List snapshots"
echo "  POST   /api/v1/instances         - Create instance"
echo "  GET    /api/v1/instances         - List instances"
echo "  GET    /health                   - Health check"
echo ""
echo "Press Ctrl+C to stop the server"
echo ""

RUST_LOG=info,faas_gateway_server=debug cargo run --package faas-gateway-server --release