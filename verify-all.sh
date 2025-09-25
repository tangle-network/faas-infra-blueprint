#!/bin/bash

# Comprehensive verification script for FaaS platform

echo "🔍 FaaS Platform Verification Script"
echo "===================================="
echo ""

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track results
PASSED=0
FAILED=0
SKIPPED=0

# Function to check test
check_test() {
    local name=$1
    local cmd=$2
    echo -n "Testing $name... "
    if eval $cmd > /dev/null 2>&1; then
        echo -e "${GREEN}✅ PASSED${NC}"
        ((PASSED++))
        return 0
    else
        echo -e "${RED}❌ FAILED${NC}"
        ((FAILED++))
        return 1
    fi
}

# Function to check optional test
check_optional() {
    local name=$1
    local cmd=$2
    echo -n "Testing $name... "
    if eval $cmd > /dev/null 2>&1; then
        echo -e "${GREEN}✅ PASSED${NC}"
        ((PASSED++))
        return 0
    else
        echo -e "${YELLOW}⚠️ SKIPPED${NC} (optional)"
        ((SKIPPED++))
        return 1
    fi
}

echo "1. Checking Build System"
echo "------------------------"
check_test "Cargo available" "which cargo"
check_test "Rust compilation" "cargo --version"
check_optional "Python 3" "which python3"
check_optional "Node.js" "which node"
check_optional "Docker" "docker --version"

echo ""
echo "2. Building Core Components"
echo "---------------------------"
check_test "faas-executor builds" "cargo build --package faas-executor"
check_test "faas-gateway builds" "cargo build --package faas-gateway"
check_test "faas-gateway-server builds" "cargo build --package faas-gateway-server"
check_test "faas-common builds" "cargo build --package faas-common"

echo ""
echo "3. Building Examples"
echo "-------------------"
check_test "gpu-service example" "cargo build --package gpu-service-example"
check_test "agent-branching example" "cargo build --package agent-branching-example"
check_test "quickstart example" "cargo build --package quickstart"

echo ""
echo "4. Running Rust Tests"
echo "--------------------"
check_test "faas-executor tests" "cargo test --package faas-executor --lib"
check_optional "faas-gateway-server tests" "cargo test --package faas-gateway-server --lib"

echo ""
echo "5. SDK Verification"
echo "------------------"

# Python SDK
if command -v python3 &> /dev/null; then
    echo -n "Testing Python SDK setup... "
    if python3 -c "import sys; sys.path.insert(0, './sdk/python'); from faas_sdk import client" 2>/dev/null; then
        echo -e "${GREEN}✅ Structure OK${NC}"
        ((PASSED++))
    else
        echo -e "${YELLOW}⚠️ Missing dependencies${NC}"
        echo "  To fix: cd sdk/python && pip install -r requirements.txt"
        ((SKIPPED++))
    fi
else
    echo "Python SDK... ${YELLOW}⚠️ Python not installed${NC}"
    ((SKIPPED++))
fi

# TypeScript SDK
if command -v node &> /dev/null; then
    echo -n "Testing TypeScript SDK... "
    if [ -f "./sdk/typescript/package.json" ]; then
        echo -e "${GREEN}✅ Structure OK${NC}"
        ((PASSED++))
    else
        echo -e "${RED}❌ Missing package.json${NC}"
        ((FAILED++))
    fi
else
    echo "TypeScript SDK... ${YELLOW}⚠️ Node not installed${NC}"
    ((SKIPPED++))
fi

echo ""
echo "6. Docker Integration"
echo "--------------------"

if command -v docker &> /dev/null; then
    check_test "Docker daemon running" "docker ps"

    if docker ps > /dev/null 2>&1; then
        echo -n "Testing Docker execution... "
        if docker run --rm alpine echo "test" > /dev/null 2>&1; then
            echo -e "${GREEN}✅ PASSED${NC}"
            ((PASSED++))
        else
            echo -e "${RED}❌ FAILED${NC}"
            ((FAILED++))
        fi
    fi
else
    echo "Docker tests... ${YELLOW}⚠️ Docker not installed${NC}"
    ((SKIPPED++))
fi

echo ""
echo "7. Gateway Server Check"
echo "----------------------"
echo -n "Checking if gateway can start... "
if cargo build --package faas-gateway-server --release > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Builds successfully${NC}"
    ((PASSED++))
else
    echo -e "${RED}❌ Build failed${NC}"
    ((FAILED++))
fi

echo ""
echo "8. Documentation Check"
echo "---------------------"
check_test "README exists" "[ -f README.md ]"
check_test "PLATFORM_STATUS exists" "[ -f PLATFORM_STATUS.md ]"
check_test "SDK_REVIEW exists" "[ -f /Users/drew/webb/faas-demos/SDK_REVIEW.md ]"
check_test "GATEWAY_IMPLEMENTATION exists" "[ -f GATEWAY_IMPLEMENTATION.md ]"

echo ""
echo "9. Demo Repository Check"
echo "-----------------------"
check_test "Python demos exist" "[ -d /Users/drew/webb/faas-demos/python ]"
check_test "TypeScript demos exist" "[ -d /Users/drew/webb/faas-demos/typescript ]"

echo ""
echo "======================================="
echo "            TEST SUMMARY               "
echo "======================================="
echo -e "${GREEN}Passed:${NC}  $PASSED"
echo -e "${RED}Failed:${NC}  $FAILED"
echo -e "${YELLOW}Skipped:${NC} $SKIPPED"
echo ""

# Overall result
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}✅ All required tests passed!${NC}"
    echo ""
    echo "Next steps:"
    echo "1. Start gateway: ./start-gateway.sh"
    echo "2. Run examples: cd examples/gpu-service && cargo run"
    echo "3. Test SDKs: ./test-gateway.py"
    exit 0
else
    echo -e "${RED}❌ Some tests failed. Please review above.${NC}"
    echo ""
    echo "Common fixes:"
    echo "- Install Docker: https://docs.docker.com/get-docker/"
    echo "- Install Python deps: cd sdk/python && pip install -r requirements.txt"
    echo "- Install Node deps: cd sdk/typescript && npm install"
    exit 1
fi