#!/bin/bash
# Test REAL blueprint deployment using incredible-squaring example
# This tests the actual FaaS platform with the compiled WASM blueprint

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}=== Testing REAL Blueprint Deployment ===${NC}"
echo "This will test the actual FaaS platform with the incredible-squaring blueprint"
echo ""

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo -e "${RED}Error: Must run from the faas project root${NC}"
    exit 1
fi

# Step 1: Build the incredible-squaring blueprint if needed
BLUEPRINT_DIR="examples/incredible-squaring"
BLUEPRINT_TARGET="$BLUEPRINT_DIR/target/wasm32-unknown-unknown/release/incredible_squaring.wasm"

echo -e "${YELLOW}Step 1: Building incredible-squaring blueprint...${NC}"
if [ ! -f "$BLUEPRINT_TARGET" ]; then
    echo "Blueprint not found, building..."
    cd $BLUEPRINT_DIR
    cargo build --release --target wasm32-unknown-unknown
    cd ../..
    if [ -f "$BLUEPRINT_TARGET" ]; then
        echo -e "${GREEN}✓ Blueprint built successfully${NC}"
        ls -lh "$BLUEPRINT_TARGET"
    else
        echo -e "${RED}✗ Failed to build blueprint${NC}"
        exit 1
    fi
else
    echo -e "${GREEN}✓ Blueprint already built${NC}"
    ls -lh "$BLUEPRINT_TARGET"
fi

# Step 2: Create a Docker container with the FaaS platform
echo -e "${YELLOW}Step 2: Creating test environment...${NC}"

cat > Dockerfile.blueprint-test << 'EOF'
FROM ubuntu:22.04

# Install dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    build-essential \
    pkg-config \
    libssl-dev \
    docker.io \
    wget \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup toolchain install nightly && \
    rustup target add wasm32-unknown-unknown

# Set working directory
WORKDIR /workspace

# Copy the entire project
COPY . .

# Build the FaaS executor
RUN cargo +nightly build --package faas-executor --release && \
    cargo +nightly build --package faas-blueprint-lib --release

# Create the test runner
RUN echo '#!/bin/bash\n\
set -e\n\
\n\
echo "=== FaaS Blueprint Deployment Test ==="\n\
echo ""\n\
\n\
# Verify the blueprint exists\n\
BLUEPRINT_PATH="/workspace/examples/incredible-squaring/target/wasm32-unknown-unknown/release/incredible_squaring.wasm"\n\
if [ ! -f "$BLUEPRINT_PATH" ]; then\n\
    echo "Building blueprint..."\n\
    cd /workspace/examples/incredible-squaring\n\
    cargo build --release --target wasm32-unknown-unknown\n\
    cd /workspace\n\
fi\n\
\n\
echo "Blueprint size: $(ls -lh $BLUEPRINT_PATH | awk '"'"'{print $5}'"'"')"\n\
echo ""\n\
\n\
# Run the actual deployment tests\n\
echo ">> Testing Blueprint Deployment"\n\
cargo +nightly test --package faas-blueprint-lib --test integration_test -- --nocapture\n\
\n\
echo ""\n\
echo ">> Testing Job Execution"\n\
cargo +nightly test --package faas-blueprint-lib execute_function -- --nocapture\n\
\n\
echo ""\n\
echo ">> Testing Platform Executor with Blueprint"\n\
export RUST_LOG=info\n\
cargo +nightly test --package faas-executor --lib platform::executor::tests::test_modes -- --ignored --nocapture\n\
' > /test-blueprint.sh && chmod +x /test-blueprint.sh

CMD ["/test-blueprint.sh"]
EOF

# Step 3: Build and run the test container
echo -e "${YELLOW}Step 3: Building test container...${NC}"
docker build -f Dockerfile.blueprint-test -t faas-blueprint-test .

# Step 4: Run the actual tests
echo -e "${YELLOW}Step 4: Running blueprint deployment tests...${NC}"
docker run --rm \
    --privileged \
    -v /var/run/docker.sock:/var/run/docker.sock \
    -e DOCKER_HOST=unix:///var/run/docker.sock \
    -e RUST_BACKTRACE=1 \
    faas-blueprint-test

if [ $? -eq 0 ]; then
    echo -e "${GREEN}=== ALL BLUEPRINT TESTS PASSED ===${NC}"
else
    echo -e "${RED}=== SOME TESTS FAILED ===${NC}"
    exit 1
fi

# Step 5: Additional verification - Run a real FaaS job
echo -e "${YELLOW}Step 5: Testing real FaaS job execution...${NC}"

# Create a test that actually deploys and runs the blueprint
cat > test-real-execution.rs << 'EOF'
use faas_blueprint_lib::context::FaaSContext;
use faas_common::SandboxConfig;
use std::sync::Arc;

#[tokio::test]
async fn test_incredible_squaring_deployment() {
    // Load the actual WASM blueprint
    let wasm_path = "examples/incredible-squaring/target/wasm32-unknown-unknown/release/incredible_squaring.wasm";
    let wasm_bytes = std::fs::read(wasm_path).expect("Failed to read WASM file");
    println!("Loaded WASM blueprint: {} bytes", wasm_bytes.len());

    // Create FaaS context
    let context = FaaSContext::try_new().await.expect("Failed to create FaaS context");

    // Deploy the blueprint
    let deployment_result = context.deploy_blueprint(wasm_bytes).await;
    assert!(deployment_result.is_ok(), "Blueprint deployment failed");

    // Execute a squaring function
    let result = context.execute_function(
        "square",
        vec![5_i32.to_le_bytes().to_vec()]
    ).await;

    assert!(result.is_ok(), "Function execution failed");

    // Verify the result (5 squared = 25)
    let output = result.unwrap();
    let squared = i32::from_le_bytes(output[..4].try_into().unwrap());
    assert_eq!(squared, 25, "Incorrect squaring result");

    println!("✓ Blueprint deployment and execution successful!");
    println!("  Input: 5, Output: {}", squared);
}
EOF

# Run the real execution test
echo -e "${BLUE}Running real blueprint execution test...${NC}"
docker run --rm \
    --privileged \
    -v /var/run/docker.sock:/var/run/docker.sock \
    -v $(pwd)/test-real-execution.rs:/workspace/test-real-execution.rs \
    -e DOCKER_HOST=unix:///var/run/docker.sock \
    faas-blueprint-test \
    bash -c "
        # Add the test to the project
        echo '
[[test]]
name = \"real_execution\"
path = \"test-real-execution.rs\"
' >> Cargo.toml

        # Run it
        cargo +nightly test --test real_execution -- --nocapture
    "

# Cleanup
rm -f Dockerfile.blueprint-test test-real-execution.rs

echo -e "${GREEN}=== Blueprint Deployment Testing Complete ===${NC}"