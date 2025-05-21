#!/bin/bash
echo "--- DOCKER ENTRYPOINT SCRIPT STARTED (stderr) ---" >&2
echo "--- DOCKER ENTRYPOINT SCRIPT STARTED (stdout) ---"

set -eo pipefail
set -x # Enable debug printing

# This script runs inside the Docker container

# --- Helper Functions ---
info() { echo "[INFO] $*" ; }
error() { echo "[ERROR] $*" >&2; exit 1; }

# --- Environment Variables ---
BUILDROOT_SRC_DIR=${BUILDROOT_SRC_DIR:-/build/buildroot-src}
PROJECT_SRC_DIR=${PROJECT_SRC_DIR:-/build/project-src}
OUTPUT_DIR=${OUTPUT_DIR:-/build/output}
OVERLAY_DIR_HOST=${OVERLAY_DIR_HOST:-/build/overlay}
CUSTOM_PACKAGE_DIR_HOST=${CUSTOM_PACKAGE_DIR_HOST:-/build/buildroot_package}
AGENT_CONTAINER_BINARY_PATH=${AGENT_CONTAINER_BINARY_PATH:-/build/project-src/target/x86_64-unknown-linux-musl/release/faas-guest-agent}
ROOTFS_IMAGE_NAME=${ROOTFS_IMAGE_NAME:-rootfs.ext4}
BUILDROOT_BASE_CONFIG_HOST=${BUILDROOT_BASE_CONFIG_HOST:-/build/buildroot_config.base}

# --- Validation ---
[ ! -d "$BUILDROOT_SRC_DIR" ] && error "Buildroot source directory not found: $BUILDROOT_SRC_DIR"
[ ! -f "$AGENT_CONTAINER_BINARY_PATH" ] && error "Agent binary not found: $AGENT_CONTAINER_BINARY_PATH"
[ ! -f "$BUILDROOT_BASE_CONFIG_HOST" ] && error "Base config not found: $BUILDROOT_BASE_CONFIG_HOST"
[ ! -d "$OVERLAY_DIR_HOST" ] && error "Overlay directory not found: $OVERLAY_DIR_HOST"
[ ! -d "$CUSTOM_PACKAGE_DIR_HOST" ] && error "Custom package directory not found: $CUSTOM_PACKAGE_DIR_HOST"

echo "--- Docker Entrypoint Start ---"
info "Running Buildroot steps inside Docker container..."

info "Buildroot source: $BUILDROOT_SRC_DIR"
info "Agent binary: $AGENT_CONTAINER_BINARY_PATH"
info "Output dir (mounted): $OUTPUT_DIR"

info "Starting Buildroot build in $OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

# Copy base config
cp "$BUILDROOT_BASE_CONFIG_HOST" "$OUTPUT_DIR/.config"

# Clean previous build artifacts
info "Running 'make clean' in output directory"
echo "--- BEFORE make clean ---"
make -C "$BUILDROOT_SRC_DIR" O="$OUTPUT_DIR" BR2_DL_DIR="$OUTPUT_DIR/buildroot-dl" V=1 clean || error "'make clean' failed"
echo "--- AFTER make clean ---"

# Configure Buildroot
info "Running 'make olddefconfig'"
echo "--- Running make olddefconfig ---"
make -C "$BUILDROOT_SRC_DIR" O="$OUTPUT_DIR" BR2_DL_DIR="$OUTPUT_DIR/buildroot-dl" olddefconfig || error "'make olddefconfig' failed"

# Build
info "Running main make"
echo "--- Running main make ---"
make -C "$BUILDROOT_SRC_DIR" O="$OUTPUT_DIR" BR2_DL_DIR="$OUTPUT_DIR/buildroot-dl" || error "Main 'make' failed"

echo "--- Finished main make ---"
info "Buildroot build completed successfully inside container."
info "Final image is in $OUTPUT_DIR/images/"

# The build process automatically places the image in $OUTPUT_DIR/images/
# We don't need an explicit copy step here as O= points to the mounted output volume. 