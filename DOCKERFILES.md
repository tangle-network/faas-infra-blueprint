# Dockerfile Documentation

This project maintains only essential Dockerfiles for production use.

## Active Dockerfiles

### 1. `/Dockerfile` (Root)
**Purpose:** Main production image using cargo-chef for optimized builds
- Uses multi-stage build with cargo-chef for dependency caching
- Includes Foundry tools for smart contract interaction
- Production runtime image

### 2. `/faas-bin/Dockerfile`
**Purpose:** Blueprint binary build for Tangle Network deployment
- Builds the faas-blueprint-bin package
- Creates minimal runtime image for Blueprint execution
- Used for blockchain-integrated deployments

### 3. `/tools/firecracker-rootfs-builder/Dockerfile`
**Purpose:** Builds custom Linux rootfs for Firecracker VMs (Linux only)
- Uses Buildroot to create minimal Linux filesystem
- Generates kernel and rootfs for Firecracker microVMs
- Only needed for Linux production deployments with Firecracker

## Removed Dockerfiles (No Longer Needed)

The following test Dockerfiles have been removed as they're replaced by the unified test runner:

- `.faas-test-x86.dockerfile` - Old x86 test image
- `crates/faas-executor/Dockerfile.test` - Old test setup
- `crates/faas-executor/Dockerfile.alpine-test` - Alpine test variant
- `crates/faas-executor/Dockerfile.minimal-test` - Minimal test image
- `crates/faas-executor/Dockerfile.criu-firecracker` - CRIU test image
- `crates/faas-executor/Dockerfile.criu-firecracker-cached` - Cached CRIU test

## Testing

Testing no longer requires Docker images. Use the consolidated test runner:

```bash
# Run all tests natively
./test-faas-platform test

# Or build a test image if needed
docker build -t faas-test .
docker run --rm -v /var/run/docker.sock:/var/run/docker.sock faas-test
```

## Building

### Production Image
```bash
docker build -t faas-platform:latest .
```

### Blueprint Binary
```bash
docker build -f faas-bin/Dockerfile -t faas-blueprint:latest .
```

### Firecracker Rootfs (Linux only)
```bash
cd tools/firecracker-rootfs-builder
./build_rootfs.sh
```