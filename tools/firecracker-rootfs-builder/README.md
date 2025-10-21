# Firecracker RootFS Builder

This directory contains the tools and configuration necessary to build a minimal Linux root filesystem (ext4) compatible with Firecracker, specifically for running the `faas-guest-agent`.

## Prerequisites

1.  **Host Build Environment:** A Linux host with standard build tools (`make`, `gcc`, `wget`, etc.).
2.  **Rust Toolchain:** A working Rust installation capable of cross-compiling for `x86_64-unknown-linux-musl`. Run `rustup target add x86_64-unknown-linux-musl`.
3.  **`faas-guest-agent` Binary:** Ensure the agent can be built successfully using `cargo build --release --package faas-guest-agent --target x86_64-unknown-linux-musl`. The build script will attempt to build it if not found.

## Usage

### Quick path (Docker, recommended)

From the repository root:

```bash
scripts/build_firecracker_rootfs.sh
```

This executes the Docker-based build and mounts the resulting artifacts under `tools/firecracker-rootfs-builder/output/`.

### Manual path (advanced/custom changes)

1.  **Navigate:** Change directory to `tools/firecracker-rootfs-builder`.
2.  **(Optional) Customize Configuration:**
    - If this is the first run or you need to modify the Buildroot configuration (e.g., add packages), copy `buildroot_config.base` to `.config` inside the Buildroot source directory (created by the script) and run `make menuconfig` within that directory.
    - Save the configuration and copy the resulting `.config` back to `buildroot_config.base` or a custom config file if desired.
3.  **Run Build Script:** Execute `./build_rootfs.sh` (requires a Linux host with kernel headers).
    - The script will download Buildroot (if needed), build the `faas-guest-agent` (if needed), configure Buildroot using `buildroot_config.base`, apply the overlay (including the `init` script), build the `ext4` rootfs image, and place it in the `output/` directory.
4.  **Set Environment Variables:** After a successful build, configure the environment variables for the FaaS service and tests:
    ```bash
    export FC_ROOTFS_PATH="$(pwd)/output/rootfs.ext4"
    # For tests:
    export TEST_FC_ROOTFS_PATH="$FC_ROOTFS_PATH"
    # Ensure FC_KERNEL_PATH/TEST_FC_KERNEL_PATH and FC_BINARY_PATH/TEST_FC_BINARY_PATH are also set.
    ```

## Structure

- `build_rootfs.sh`: Main automation script.
- `buildroot_config.base`: Base Buildroot configuration targeting `x86_64`, `musl`, `ext4`, and including the custom `faas-guest-agent`.
- `overlay/`: Files overlaid onto the target root filesystem.
  - `init`: Simple init script to mount essential filesystems and start the `faas-guest-agent`.
- `buildroot_package/`: Custom Buildroot package definitions.
  - `faas-guest-agent/`: Package for the pre-compiled guest agent.
- `output/`: Default location for the final `rootfs.ext4` image.
- `buildroot-<version>/`: Buildroot source and build directory (created by the script).

## Kernel (`vmlinux.bin`)

This builder _only_ creates the root filesystem. You still need to obtain a compatible Firecracker kernel binary (`vmlinux.bin`) separately.

- **Recommendation:** Download a pre-built kernel from the official Firecracker releases matching the version of the Firecracker binary you intend to use.
- **Configuration:** Set the `FC_KERNEL_PATH` (and `TEST_FC_KERNEL_PATH`) environment variable to the _absolute path_ of your `vmlinux.bin` file.
