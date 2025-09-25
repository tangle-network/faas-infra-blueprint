#!/bin/bash
# Test script for FaaS examples

set -e

echo "🧪 Testing FaaS Examples"
echo "========================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to run an example
run_example() {
    local name=$1
    local path=$2

    echo ""
    echo -e "${YELLOW}Testing: $name${NC}"
    echo "------------------------"

    if cargo +nightly build --manifest-path "$path/Cargo.toml" 2>&1 | tail -5; then
        echo -e "${GREEN}✅ $name builds successfully${NC}"
        return 0
    else
        echo -e "${RED}❌ $name failed to build${NC}"
        return 1
    fi
}

# Test if Docker is available
if ! docker version > /dev/null 2>&1; then
    echo -e "${RED}❌ Docker is not running. Examples require Docker.${NC}"
    exit 1
fi

echo "✅ Docker is available"

# Build all examples
echo ""
echo "Building all examples..."
echo ""

success_count=0
failure_count=0

# Test each example
examples=(
    "quickstart:examples/quickstart"
    "gpu-service:examples/gpu-service"
    "agent-branching:examples/agent-branching"
    "zk-faas:examples/zk-faas"
    "faas-sdk:examples/faas-sdk"
    "showcase:examples/showcase"
    "remote-dev:examples/remote-dev"
)

for example in "${examples[@]}"; do
    IFS=':' read -r name path <<< "$example"
    if run_example "$name" "$path"; then
        ((success_count++))
    else
        ((failure_count++))
    fi
done

echo ""
echo "========================"
echo "📊 Results Summary"
echo "========================"
echo -e "${GREEN}✅ Successful builds: $success_count${NC}"
echo -e "${RED}❌ Failed builds: $failure_count${NC}"

if [ $failure_count -eq 0 ]; then
    echo ""
    echo -e "${GREEN}🎉 All examples build successfully!${NC}"
    exit 0
else
    echo ""
    echo -e "${RED}⚠️  Some examples failed to build${NC}"
    exit 1
fi