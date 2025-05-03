#!/bin/bash
set -eo pipefail

# This script runs inside the Docker container

# --- Configuration (passed via environment variables or defaults) ---
BUILDROOT_SRC_DIR="/build/buildroot-src"
PROJECT_SRC_DIR="/build/project-src" # Mounted read-only usually
OUTPUT_DIR="/build/output"            # Mounted read-write
OVERLAY_DIR_HOST="/build/overlay"        # Path to overlay dir as copied into image
CUSTOM_PACKAGE_DIR_HOST="/build/buildroot_package" # Path to custom pkg dir as copied into image

# Agent path needs to be relative to the mounted project source
AGENT_CONTAINER_BINARY_PATH="${AGENT_BINARY_PATH:-/build/project-src/target/x86_64-unknown-linux-musl/release/faas-guest-agent}"

ROOTFS_IMAGE_NAME="${ROOTFS_IMAGE_NAME:-rootfs.ext4}"
BUILDROOT_BASE_CONFIG_HOST="/build/buildroot_config.base"

# --- Helper Functions ---
info() { echo "[INFO] $*" >&2; } # Log to stderr inside container
error() { echo "[ERROR] $*" >&2; exit 1; }

# --- Pre-checks ---
if [ ! -d "$BUILDROOT_SRC_DIR" ] || [ -z "$(ls -A $BUILDROOT_SRC_DIR)" ]; then
    error "Buildroot source directory ($BUILDROOT_SRC_DIR) is not mounted or empty."
fi
if [ ! -f "$AGENT_CONTAINER_BINARY_PATH" ]; then
    error "Agent binary ($AGENT_CONTAINER_BINARY_PATH) not found within mounted project source."
fi
if [ ! -f "$BUILDROOT_BASE_CONFIG_HOST" ]; then
    error "Base Buildroot config ($BUILDROOT_BASE_CONFIG_HOST) not found."
fi
if [ ! -d "$OVERLAY_DIR_HOST" ]; then
    error "Overlay directory ($OVERLAY_DIR_HOST) not found."
fi
if [ ! -d "$CUSTOM_PACKAGE_DIR_HOST" ]; then
    error "Custom package directory ($CUSTOM_PACKAGE_DIR_HOST) not found."
fi

info "Running Buildroot steps inside Docker container..."
info "Buildroot source: $BUILDROOT_SRC_DIR"
info "Agent binary: $AGENT_CONTAINER_BINARY_PATH"
info "Output dir (mounted): $OUTPUT_DIR"

# --- Build Steps Inside Container ---

info "Starting Buildroot build in ${OUTPUT_DIR}"

# 1. Ensure output dir exists and copy config
mkdir -p "$OUTPUT_DIR"
cp "$BUILDROOT_BASE_CONFIG_HOST" "$OUTPUT_DIR/.config" || error "Failed to copy base config to ${OUTPUT_DIR}/.config"

# 2. Configure Buildroot using olddefconfig within the output directory
info "Running 'make olddefconfig'"
make -C "$BUILDROOT_SRC_DIR" O="$OUTPUT_DIR" olddefconfig || error "'make olddefconfig' failed"

# 3. Run the main build
info "Running main 'make'"
# Initially, run without external package or overlay variables
make -C "$BUILDROOT_SRC_DIR" O="$OUTPUT_DIR" -j$(nproc) || error "Main 'make' failed"

info "Buildroot build completed successfully inside container."
info "Final image is in ${OUTPUT_DIR}/images/"

# The build process automatically places the image in $OUTPUT_DIR/images/
# We don't need an explicit copy step here as O= points to the mounted output volume. 