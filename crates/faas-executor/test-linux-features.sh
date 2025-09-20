#!/bin/bash
# Script to test Linux-specific features (CRIU, Firecracker) on macOS using Docker

set -e

echo "=== Testing Linux Features in Docker ==="
echo "This will test CRIU and Firecracker functionality using Docker containers"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to run a test service
run_test() {
    local service=$1
    local description=$2

    echo -e "${YELLOW}Testing: $description${NC}"
    docker-compose -f docker-compose.test.yml up --build $service

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ $description passed${NC}"
    else
        echo -e "${RED}✗ $description failed${NC}"
    fi
    echo ""
}

# Build the test image first
echo "Building test Docker image..."
docker-compose -f docker-compose.test.yml build

# Option 1: Run CRIU tests
if [ "$1" == "criu" ] || [ -z "$1" ]; then
    run_test "criu-tests" "CRIU Checkpoint/Restore Tests"
fi

# Option 2: Run Firecracker mock tests
if [ "$1" == "firecracker" ] || [ -z "$1" ]; then
    run_test "firecracker-mock" "Firecracker Mock Tests"
fi

# Option 3: Run all integration tests
if [ "$1" == "integration" ] || [ -z "$1" ]; then
    run_test "integration-tests" "Full Integration Test Suite"
fi

# Option 4: Interactive debugging
if [ "$1" == "debug" ]; then
    echo "Starting interactive debug container..."
    docker-compose -f docker-compose.test.yml run --rm criu-tests bash
fi

echo -e "${GREEN}=== Test run complete ===${NC}"

# Cleanup
if [ "$2" != "--no-cleanup" ]; then
    echo "Cleaning up containers..."
    docker-compose -f docker-compose.test.yml down
fi