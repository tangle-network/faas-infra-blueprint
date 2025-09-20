#!/bin/bash
# Build and run tests with Docker caching

set -e

# Enable BuildKit for better caching
export DOCKER_BUILDKIT=1
export COMPOSE_DOCKER_CLI_BUILD=1
export DOCKER_HOST=unix:///var/run/docker.sock

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}=== FaaS Test Runner with Caching ===${NC}"
echo ""

# Function to build with cache
build_with_cache() {
    echo -e "${YELLOW}Building Docker image with cache...${NC}"

    # Build the image with BuildKit caching
    docker build \
        --cache-from faas-executor-cached:latest \
        --build-arg BUILDKIT_INLINE_CACHE=1 \
        -f crates/faas-executor/Dockerfile.criu-firecracker-cached \
        -t faas-executor-cached:latest \
        --target runtime \
        . 2>&1 | grep -E "(CACHED|DONE|ERROR)" || true

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Build completed (using cache where possible)${NC}"
    else
        echo -e "${RED}✗ Build failed${NC}"
        exit 1
    fi
}

# Function to run specific test suite
run_test_suite() {
    local suite=$1
    local description=$2

    echo -e "${YELLOW}Running: $description${NC}"

    docker-compose -f docker-compose.cached-test.yml run --rm $suite

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ $description passed${NC}"
    else
        echo -e "${RED}✗ $description failed${NC}"
    fi
}

# Parse arguments
case "$1" in
    build)
        build_with_cache
        ;;
    docker)
        build_with_cache
        run_test_suite "docker-tests" "Docker Integration Tests"
        ;;
    criu)
        build_with_cache
        run_test_suite "criu-tests" "CRIU Tests"
        ;;
    quick)
        build_with_cache
        run_test_suite "quick-test" "Quick Tests"
        ;;
    all)
        build_with_cache
        run_test_suite "quick-test" "Quick Tests"
        run_test_suite "docker-tests" "Docker Integration Tests"
        if [ "$(uname)" = "Linux" ]; then
            run_test_suite "criu-tests" "CRIU Tests"
        fi
        ;;
    shell)
        echo -e "${YELLOW}Starting interactive shell...${NC}"
        docker-compose -f docker-compose.cached-test.yml run --rm quick-test bash
        ;;
    *)
        echo "Usage: $0 {build|docker|criu|quick|all|shell}"
        echo ""
        echo "  build  - Build Docker image with caching"
        echo "  docker - Run Docker integration tests"
        echo "  criu   - Run CRIU tests (Linux only)"
        echo "  quick  - Run quick unit tests"
        echo "  all    - Run all test suites"
        echo "  shell  - Start interactive shell in container"
        exit 1
        ;;
esac

echo -e "${GREEN}=== Done ===${NC}"