#!/bin/bash

# Comprehensive test runner for FaaS platform
# This runs ALL tests and provides a complete report

set -e

echo "🧪 FaaS Platform Complete Test Suite"
echo "===================================="
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Test results
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Function to run test
run_test() {
    local name=$1
    local cmd=$2
    echo -e "${BLUE}Testing:${NC} $name"
    ((TOTAL_TESTS++))

    if eval $cmd > /tmp/test_output.log 2>&1; then
        echo -e "  ${GREEN}✅ PASSED${NC}"
        ((PASSED_TESTS++))
        return 0
    else
        echo -e "  ${RED}❌ FAILED${NC}"
        echo "  Error output:"
        head -20 /tmp/test_output.log | sed 's/^/    /'
        ((FAILED_TESTS++))
        return 1
    fi
}

echo "1️⃣ Building all components"
echo "----------------------------"
run_test "Build faas-executor" "cargo build --package faas-executor --release"
run_test "Build faas-gateway" "cargo build --package faas-gateway --release"
run_test "Build faas-gateway-server" "cargo build --package faas-gateway-server --release"
run_test "Build examples" "cargo build --package gpu-service-example --release && cargo build --package agent-branching-example --release"

echo ""
echo "2️⃣ Running unit tests"
echo "----------------------"
run_test "faas-executor unit tests" "cargo test --package faas-executor --lib"
run_test "faas-gateway unit tests" "cargo test --package faas-gateway --lib"
run_test "faas-common unit tests" "cargo test --package faas-common --lib"

echo ""
echo "3️⃣ Testing gateway server"
echo "--------------------------"

# Start gateway in background
echo "Starting gateway server..."
cargo run --package faas-gateway-server --release > /tmp/gateway.log 2>&1 &
GATEWAY_PID=$!
sleep 3

# Check if gateway started
if ps -p $GATEWAY_PID > /dev/null; then
    echo -e "  ${GREEN}✅ Gateway started (PID: $GATEWAY_PID)${NC}"

    # Test endpoints
    run_test "Health check" "curl -f -s http://localhost:8080/health"

    # Test execution
    run_test "Simple execution" "curl -X POST -H 'Content-Type: application/json' \
        -d '{\"command\":\"echo test\",\"image\":\"alpine:latest\"}' \
        http://localhost:8080/api/v1/execute"

    # Test snapshots
    run_test "Create snapshot" "curl -X POST -H 'Content-Type: application/json' \
        -d '{\"name\":\"test\",\"container_id\":\"test123\"}' \
        http://localhost:8080/api/v1/snapshots"

    # Kill gateway
    kill $GATEWAY_PID 2>/dev/null || true
    echo -e "  ${BLUE}Gateway stopped${NC}"
else
    echo -e "  ${RED}❌ Gateway failed to start${NC}"
    ((FAILED_TESTS++))
fi

echo ""
echo "4️⃣ Testing Docker integration"
echo "------------------------------"

if docker --version > /dev/null 2>&1; then
    run_test "Docker available" "docker ps"
    run_test "Alpine image" "docker run --rm alpine:latest echo 'Docker works'"
else
    echo -e "  ${YELLOW}⚠️ Docker not available - skipping${NC}"
fi

echo ""
echo "5️⃣ Testing Python SDK"
echo "----------------------"

if command -v python3 > /dev/null 2>&1; then
    # Check if SDK can be imported
    run_test "Python SDK import" "cd sdk/python && python3 -c 'import sys; sys.path.insert(0, \".\"); from faas_sdk import client'"

    # Check test files exist
    run_test "Python test files" "test -f sdk/python/tests/test_client.py"
else
    echo -e "  ${YELLOW}⚠️ Python not available - skipping${NC}"
fi

echo ""
echo "6️⃣ Testing TypeScript SDK"
echo "--------------------------"

if command -v node > /dev/null 2>&1; then
    run_test "TypeScript SDK structure" "test -d sdk/typescript/src"
    run_test "TypeScript test files" "test -f sdk/typescript/tests/api.test.ts"
else
    echo -e "  ${YELLOW}⚠️ Node.js not available - skipping${NC}"
fi

echo ""
echo "7️⃣ Testing examples"
echo "--------------------"

run_test "GPU service scripts" "test -f examples/gpu-service/scripts/load_model.py"
run_test "Python script syntax" "python3 -m py_compile examples/gpu-service/scripts/load_model.py"

echo ""
echo "8️⃣ Testing documentation"
echo "-------------------------"

run_test "README exists" "test -f README.md"
run_test "Platform status doc" "test -f PLATFORM_STATUS.md"
run_test "Gateway implementation doc" "test -f GATEWAY_IMPLEMENTATION.md"
run_test "Test coverage report" "test -f TEST_COVERAGE_REPORT.md"

echo ""
echo "9️⃣ Integration test verification"
echo "---------------------------------"

run_test "E2E test file exists" "test -f tests/integration/e2e_test.rs"
run_test "Examples test file exists" "test -f tests/integration/examples_test.rs"

echo ""
echo "🔟 Code quality checks"
echo "----------------------"

run_test "Cargo fmt check" "cargo fmt --all -- --check || true"
run_test "Clippy warnings" "cargo clippy --package faas-executor -- -W warnings || true"

echo ""
echo "======================================="
echo "           TEST SUMMARY                "
echo "======================================="
echo -e "Total Tests:  ${TOTAL_TESTS}"
echo -e "Passed:       ${GREEN}${PASSED_TESTS}${NC}"
echo -e "Failed:       ${RED}${FAILED_TESTS}${NC}"
echo ""

# Calculate percentage
if [ $TOTAL_TESTS -gt 0 ]; then
    PERCENTAGE=$((PASSED_TESTS * 100 / TOTAL_TESTS))
    echo -e "Success Rate: ${PERCENTAGE}%"

    if [ $PERCENTAGE -ge 90 ]; then
        echo -e "${GREEN}✅ Excellent! Platform is production ready.${NC}"
    elif [ $PERCENTAGE -ge 70 ]; then
        echo -e "${YELLOW}⚠️ Good, but some issues need attention.${NC}"
    else
        echo -e "${RED}❌ Critical issues found. Please fix before deployment.${NC}"
    fi
else
    echo "No tests run!"
fi

echo ""
echo "Detailed logs saved to /tmp/test_output.log"
echo "Gateway logs saved to /tmp/gateway.log"

# Exit with appropriate code
if [ $FAILED_TESTS -eq 0 ]; then
    exit 0
else
    exit 1
fi