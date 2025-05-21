#!/bin/bash
# Build script for Firecracker root filesystem using Docker

set -eo pipefail
# set -x # Uncomment for debugging

# --- Configuration ---
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
PROJECT_ROOT=$(realpath "$SCRIPT_DIR/../..")

# Buildroot configuration
BUILDROOT_VERSION="2024.02.3"
BUILDROOT_SRC_DIR="${SCRIPT_DIR}/buildroot-${BUILDROOT_VERSION}"
BUILDROOT_DOWNLOAD_URL="https://buildroot.org/downloads/buildroot-${BUILDROOT_VERSION}.tar.gz"
BUILDROOT_TARBALL="${SCRIPT_DIR}/buildroot-${BUILDROOT_VERSION}.tar.gz"
# Base config still used by host script if needed, but primarily used inside container
BUILDROOT_BASE_CONFIG="${SCRIPT_DIR}/buildroot_config.base"

# FaaS Guest Agent configuration (needed on host for build step)
AGENT_PKG_NAME="faas-guest-agent"
AGENT_CRATE_PATH="${PROJECT_ROOT}/crates/${AGENT_PKG_NAME}"
AGENT_TARGET_TRIPLE="x86_64-unknown-linux-musl"
AGENT_BINARY_NAME="${AGENT_PKG_NAME}"
AGENT_BUILD_PROFILE="release"
AGENT_HOST_BINARY_PATH="${PROJECT_ROOT}/target/${AGENT_TARGET_TRIPLE}/${AGENT_BUILD_PROFILE}/${AGENT_BINARY_NAME}"
# Path expected inside the container (relative to project root mount)
AGENT_CONTAINER_BINARY_PATH="/build/project-src/target/${AGENT_TARGET_TRIPLE}/${AGENT_BUILD_PROFILE}/${AGENT_BINARY_NAME}"

# Output configuration (on host)
OUTPUT_DIR="${SCRIPT_DIR}/output"
ROOTFS_IMAGE_NAME="rootfs.ext4"
ROOTFS_IMAGE_PATH="${OUTPUT_DIR}/${ROOTFS_IMAGE_NAME}"

# Overlay configuration (on host)
OVERLAY_DIR="${SCRIPT_DIR}/overlay"
INIT_SCRIPT_PATH="${OVERLAY_DIR}/init"

# Custom Buildroot packages (on host)
CUSTOM_PACKAGE_DIR="${SCRIPT_DIR}/buildroot_package"

# Docker configuration
DOCKER_IMAGE_TAG="faas/rootfs-builder:${BUILDROOT_VERSION}"
DOCKERFILE_PATH="${SCRIPT_DIR}/Dockerfile"

# --- Helper Functions ---
info() { echo "[INFO] $*" ; }
error() { echo "[ERROR] $*" >&2; exit 1; }

# --- Build Steps --- (Host-side preparations)

# 1. Build FaaS Guest Agent (Host)
build_guest_agent() {
    info "Checking for FaaS Guest Agent binary on host..."
    if [ -f "$AGENT_HOST_BINARY_PATH" ]; then
        if [ "$AGENT_CRATE_PATH/src/main.rs" -nt "$AGENT_HOST_BINARY_PATH" ]; then
             info "Agent source is newer than binary. Rebuilding..."
        else
            info "Using existing agent binary: ${AGENT_HOST_BINARY_PATH}"
            return 0
        fi
    fi
    info "Building FaaS Guest Agent (target: ${AGENT_TARGET_TRIPLE}) on host..."
    if ! command -v cargo &>/dev/null; then error "Cargo not found. Please install Rust."; fi
    if ! rustup target list --installed | grep -q "${AGENT_TARGET_TRIPLE}"; then
        info "Rust target ${AGENT_TARGET_TRIPLE} not found. Installing..."
        rustup target add "${AGENT_TARGET_TRIPLE}" || error "Failed to install Rust target"
    fi
    (cd "$PROJECT_ROOT" && cargo build --profile "${AGENT_BUILD_PROFILE}" --package "${AGENT_PKG_NAME}" --target "${AGENT_TARGET_TRIPLE}") || error "Failed to build FaaS Guest Agent"
    if [ ! -f "$AGENT_HOST_BINARY_PATH" ]; then
        error "Agent binary not found after build: ${AGENT_HOST_BINARY_PATH}"
    fi
    info "FaaS Guest Agent built successfully: ${AGENT_HOST_BINARY_PATH}"
}

# 2. Setup Buildroot Source (Host)
setup_buildroot() {
    info "Setting up Buildroot source version ${BUILDROOT_VERSION} on host..."
    if [ ! -d "$BUILDROOT_SRC_DIR" ]; then
        if [ ! -f "$BUILDROOT_TARBALL" ]; then
            info "Downloading Buildroot source..."
            wget --progress=dot:giga -O "$BUILDROOT_TARBALL" "$BUILDROOT_DOWNLOAD_URL" || error "Failed download"
        fi
        info "Extracting Buildroot source..."
        tar -xzf "$BUILDROOT_TARBALL" -C "$SCRIPT_DIR" || error "Failed extract"
        rm -f "$BUILDROOT_TARBALL"
        info "Buildroot source extracted to ${BUILDROOT_SRC_DIR}"
    else
        info "Buildroot source directory already exists: ${BUILDROOT_SRC_DIR}"
    fi
}

# 3. Build Docker Image (Host)
build_docker_image() {
    info "Checking for Docker image ${DOCKER_IMAGE_TAG}..."
    # Always rebuild if Dockerfile is newer than the image
    if ! docker image inspect "${DOCKER_IMAGE_TAG}" &> /dev/null || [ "${DOCKERFILE_PATH}" -nt "$(docker image inspect -f '{{.Created}}' "${DOCKER_IMAGE_TAG}")" ]; then
        info "Building Docker image ${DOCKER_IMAGE_TAG} (forcing --no-cache and --progress=plain for agent binary inclusion, context: PROJECT_ROOT)..."
        docker build --pull --no-cache --progress=plain -t "${DOCKER_IMAGE_TAG}" \
            --build-arg BUILDROOT_VERSION="${BUILDROOT_VERSION}" \
            -f "${DOCKERFILE_PATH}" "${PROJECT_ROOT}" || error "Docker image build failed"
        info "Docker image built."
    else
        info "Docker image found and up-to-date."
    fi
}

# 4. Run Buildroot inside Docker (Host invokes Container)
run_buildroot_in_docker() {
    info "Running Buildroot build process inside Docker container..."
    if ! command -v docker &>/dev/null; then error "Docker command not found. Please install Docker."; fi

    mkdir -p "$OUTPUT_DIR"

    info "Preparing Docker run command..."
    local docker_run_cmd="docker run --rm --init \
        --user \"$(id -u):$(id -g)\" \
        -v \"${OUTPUT_DIR}:/build/output:rw\" \
        -e \"AGENT_CONTAINER_BINARY_PATH=${AGENT_CONTAINER_BINARY_PATH}\" \
        -e \"ROOTFS_IMAGE_NAME=${ROOTFS_IMAGE_NAME}\" \
        \"${DOCKER_IMAGE_TAG}\""

    info "Executing Docker command:"
    echo "$docker_run_cmd"

    # Run the container, mounting ONLY the output directory
    info "Starting Docker container (with output-only mount)..."
    eval "$docker_run_cmd"
    local docker_exit_code=$?

    if [ $docker_exit_code -ne 0 ]; then
        error "Docker container exited with error code: $docker_exit_code. Check container logs for details (e.g., docker logs <container_id_if_not_removed_by_--rm>)."
    fi
    info "Docker container finished with exit code: $docker_exit_code"

    # Post-run checks remain the same
    local final_image_in_output="${OUTPUT_DIR}/images/${ROOTFS_IMAGE_NAME}"
    if [ ! -f "${final_image_in_output}" ]; then
         error "Final rootfs image not found in output/images/ after Docker run: ${final_image_in_output}"
    fi
    mv "${final_image_in_output}" "${ROOTFS_IMAGE_PATH}" || error "Failed to move final image to ${ROOTFS_IMAGE_PATH}"

    info "Build process finished successfully!"
    info "Root filesystem image created: ${ROOTFS_IMAGE_PATH}"
}

# --- Main Execution --- (Host)
main() {
    build_guest_agent
    setup_buildroot
    build_docker_image
    run_buildroot_in_docker

    info "-----------------------------------------------------"
    info "SUCCESS! Rootfs built."
    info "Path: ${ROOTFS_IMAGE_PATH}"
    info "Remember to set FC_ROOTFS_PATH environment variable."
    info "-----------------------------------------------------"
}

main 