#!/bin/bash
# Integration test runner for Python SDK
#
# This script:
# 1. Starts the faas-gateway-server in the background
# 2. Waits for it to be ready
# 3. Runs integration tests
# 4. Stops the server
#
# Usage:
#   ./run-integration-tests.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Starting FaaS Gateway Server...${NC}"

# Start the gateway server in the background
cd ../../
cargo build --package faas-gateway-server --release
RUST_LOG=info cargo run --package faas-gateway-server --release &
GATEWAY_PID=$!

# Save PID for cleanup
echo $GATEWAY_PID > /tmp/faas-gateway-test.pid

# Function to cleanup on exit
cleanup() {
  echo -e "${YELLOW}Stopping gateway server...${NC}"
  if [ -f /tmp/faas-gateway-test.pid ]; then
    kill $(cat /tmp/faas-gateway-test.pid) 2>/dev/null || true
    rm /tmp/faas-gateway-test.pid
  fi
}

trap cleanup EXIT

# Wait for server to be ready
echo -e "${YELLOW}Waiting for gateway to be ready...${NC}"
for i in {1..30}; do
  if curl -s http://localhost:8080/health > /dev/null 2>&1; then
    echo -e "${GREEN}Gateway is ready!${NC}"
    break
  fi
  if [ $i -eq 30 ]; then
    echo -e "${RED}Gateway failed to start within 30 seconds${NC}"
    exit 1
  fi
  sleep 1
done

# Run integration tests
cd sdks/python
echo -e "${YELLOW}Running integration tests...${NC}"

# Install dependencies if needed
if [ ! -d "test_env" ]; then
  python3 -m venv test_env
  source test_env/bin/activate
  pip install -e .
  pip install pytest pytest-asyncio
else
  source test_env/bin/activate
fi

pytest tests/test_integration.py -v

echo -e "${GREEN}Integration tests completed successfully!${NC}"
