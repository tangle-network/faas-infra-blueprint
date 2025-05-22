# FaaS Platform on Tangle

## Overview

This project implements a Function-as-a-Service (FaaS) platform designed to run untrusted code securely within isolated environments, orchestrated via Tangle interactions (though primarily exposing an HTTP gateway currently). It leverages technologies like Firecracker for microVM isolation, providing a robust foundation for executing arbitrary functions triggered by external requests.

The platform is structured as a collection of Rust crates, enabling modular development and testing.

## Architecture & Packages

The repository is organized into the following key crates:

- **`faas-common`**: Contains shared data structures (e.g., `InvocationResult`, `SandboxConfig`), types, constants, and error definitions used across multiple crates.
- **`faas-guest-agent`**: A small binary designed to run _inside_ the execution environment (specifically Firecracker microVMs). It listens for invocation details (command, payload) via vsock, executes the requested command, captures its stdout/stderr, and sends the `InvocationResult` back to the host executor. It is statically compiled against musl libc for portability.
- **`faas-executor`**: Implements the executor interface for running functions within isolated environments.
  - **`FirecrackerExecutor`**: Manages the lifecycle of Firecracker microVMs (creation, starting, stopping, cleanup), interacts with the `faas-guest-agent` via vsock, and requires a pre-built Firecracker-compatible kernel and root filesystem containing the guest agent.
  - **`DockerExecutor`**: Implements the executor interface for running functions within Docker containers.
- **`faas-orchestrator`**: The core component responsible for receiving function invocation requests. It selects the appropriate executor (e.g., Firecracker, Docker) based on configuration or request details, manages the execution lifecycle via the chosen executor, handles timeouts/errors, and aggregates the results.
- **`faas-gateway`**: Provides the public-facing HTTP API (built with Axum) for invoking functions. It receives user requests, forwards them to the `faas-orchestrator`, and returns the results as JSON responses.
- **`faas-lib`**: Contains core library components, potentially shared logic used by the orchestrator or other parts of the system. (This might also be referred to as `faas-blueprint-lib` in some build outputs).
- **`faas-bin`**: The main executable binary. It initializes and runs the necessary services, likely including the `faas-gateway` and the `faas-orchestrator` with configured executors. (Note: While potentially deployable as a Tangle Blueprint, the primary user interaction model currently described is via the HTTP gateway).
- **`faas-tester`**: Holds integration tests and end-to-end tests for verifying the functionality of the gateway, orchestrator, and executors.
- **`tools/firecracker-rootfs-builder`**: Contains scripts and configuration (using Buildroot via Docker) to build the minimal `rootfs.ext4` filesystem image required by the `FirecrackerExecutor`.

## Getting Started

### Prerequisites

1.  **Rust Toolchain:** Install Rust (latest stable recommended) via `rustup`.
2.  **Docker:** Required for building the Firecracker rootfs using the provided script and for using the `DockerExecutor`.
3.  **(For Firecracker Executor):**
    - `firecracker` binary accessible in your `PATH`.
    - Firecracker-compatible kernel (`vmlinux.bin`).
    - Build the root filesystem (see below).
    - **On macOS:** Firecracker requires a Linux environment. See the "macOS Development Setup" section below.

### Building

1.  **Build Firecracker RootFS:**
    ```bash
    cd tools/firecracker-rootfs-builder
    ./build_rootfs.sh
    # Note the output path, e.g., tools/firecracker-rootfs-builder/output/rootfs.ext4
    cd ../..
    ```
2.  **Build the Main Binary:**
    ```bash
    cargo build --release --package faas-bin
    ```

## Running the Service

To run the main FaaS service (which includes the gateway and orchestrator):

1.  **Configure Environment Variables:**
    Create a `.env` file in the root of the project or export the following environment variables. The `faas-bin` executable uses `dotenvy` to load these at startup.

    ```bash
    # General Configuration
    FAAS_EXECUTOR_TYPE="firecracker" # or "docker"
    # FAAS_GATEWAY_PORT=8080 # Default is typically 8080 or 3000, check faas-gateway main.rs
    # RUST_LOG=info,faas_gateway=debug # Adjust log levels as needed

    # Firecracker Executor Configuration (if FAAS_EXECUTOR_TYPE="firecracker")
    # Path to the Firecracker binary
    FAAS_FC_BINARY_PATH="/path/to/your/firecracker"
    # Path to the compatible Linux kernel
    FAAS_FC_KERNEL_PATH="/path/to/your/vmlinux.bin"
    # Path to the default rootfs built by the script
    FAAS_FC_ROOTFS_PATH="/path/to/your/faas/tools/firecracker-rootfs-builder/output/rootfs.ext4"

    # Docker Executor Configuration (if FAAS_EXECUTOR_TYPE="docker")
    # DOCKER_HOST="unix:///var/run/docker.sock" # Or your Docker daemon address
    ```

2.  **Run the Binary:**
    ```bash
    ./target/release/faas-bin
    ```
    The service will start, and the HTTP gateway will listen on the configured port (default likely 8080 or 3000, check gateway code).

## User Interaction (HTTP Gateway API)

Users interact with the deployed service primarily through the HTTP gateway.

### Invoke Function

- **Endpoint:** `POST /functions/{function_id}/invoke`
- **Purpose:** Executes a command within an isolated environment as defined by the `SandboxConfig` in the request body. The `{function_id}` path parameter is currently used for logging/tracing and is included in the `InvocationResult`.
- **Request Body (JSON - `SandboxConfig`):**

  ```json
  {
    "image": "your-docker-image:latest", // Used by DockerExecutor. Ignored by FirecrackerExecutor if a fixed rootfs is used.
    "command": ["/path/to/executable_in_guest", "arg1", "arg2"], // Command and arguments to run inside the guest
    "env_vars": ["VAR1=value1", "VAR2=value2"], // Optional: Environment variables for the command
    "payload_base64": "SGVsbG8gd29ybGQh" // Base64 encoded raw bytes to be piped to the command's stdin
  }
  ```

  _Note: The `payload_base64` field should contain a Base64 encoded string of the raw bytes for stdin._

- **Success Response (200 OK, JSON - `InvocationResult`):** Returns the execution result.

  ```json
  {
    "request_id": "some-unique-request-id", // The {function_id} from the path
    "response_base64": "T3V0cHV0IHdyaXR0ZW4gdG8gc3Rkb3V0...", // Base64 encoded stdout from the command
    "logs_base64": "Q29tYmluZWQgc3Rkb3V0IGFuZCBzdGRlcnIuLi4=", // Base64 encoded combined stdout and stderr
    "error": null // Null if the command executed successfully from the guest agent's perspective
  }
  ```

- **Execution Error Response (200 OK, JSON - `InvocationResult`):** If the command ran but the _guest agent_ or _executor_ reported an execution failure (e.g., command not found, non-zero exit code).

  ```json
  {
    "request_id": "some-unique-request-id",
    "response_base64": null, // Or potentially some partial output
    "logs_base64": "RXJyb3IgbWVzc2FnZSBmcm9tIGFnZW50Lg==",
    "error": "Error message reported by guest agent or executor (e.g., 'Command exited with non-zero status: 1')"
  }
  ```

- **Gateway/Orchestrator Error Response (4xx/5xx, JSON):** If there's an issue processing the request _before or during_ orchestration (e.g., bad request format, orchestrator failure, timeout).
  ```json
  {
    "error": "Specific error message (e.g., Bad Request: ..., Internal server error)"
  }
  ```

### Job Execution Flow (Firecracker)

1.  Client sends `POST /functions/{id}/invoke` request (with `SandboxConfig` JSON body) to `faas-gateway`.
2.  Gateway parses the `SandboxConfig` and calls `Orchestrator::schedule_execution` with it.
3.  Orchestrator validates the request and, if `FAAS_EXECUTOR_TYPE` is "firecracker", selects the `FirecrackerExecutor`.
4.  `FirecrackerExecutor` sets up a new Firecracker microVM using the configured kernel (`FAAS_FC_KERNEL_PATH`) and rootfs (`FAAS_FC_ROOTFS_PATH`).
5.  The microVM boots, and the `/init` script inside the rootfs starts the `faas-guest-agent`.
6.  The guest agent establishes communication with the host executor via vsock.
7.  The executor serializes the `SandboxConfig` to JSON and sends it to the guest agent via vsock.
8.  The guest agent deserializes the `SandboxConfig`, sets up the environment, decodes the `payload_base64` to raw bytes, executes the specified `command` with `env_vars`, pipes the decoded payload to its stdin, and captures its stdout and stderr.
9.  Upon command completion, the guest agent packages the `response` (stdout from the command, base64 encoded), `logs` (combined stdout/stderr, base64 encoded), and any execution `error` message into an `InvocationResult` structure (also including the `request_id`) and sends it back to the host executor via vsock.
10. The executor receives the `InvocationResult`, signals the microVM to shut down, and cleans up resources.
11. The executor returns the `InvocationResult` to the orchestrator.
12. The orchestrator returns the result to the gateway.
13. The gateway serializes the `InvocationResult` to JSON and sends the HTTP response to the client.

## Configuration

Key environment variables (can be set in a `.env` file):

- `FAAS_EXECUTOR_TYPE`: Specifies the executor to use. E.g., "firecracker", "docker".
- `FAAS_GATEWAY_PORT`: Port for the HTTP gateway (e.g., `8080`). Check `faas-gateway/src/main.rs` for the default if not set.
- `RUST_LOG`: Controls logging level (e.g., `info`, `debug`, `faas_gateway=debug,warn`).

**For Firecracker Executor (`FAAS_EXECUTOR_TYPE="firecracker"`):**

- `FAAS_FC_BINARY_PATH`: Absolute path to the `firecracker` executable.
- `FAAS_FC_KERNEL_PATH`: Absolute path to the `vmlinux.bin` kernel file.
- `FAAS_FC_ROOTFS_PATH`: Absolute path to the default `rootfs.ext4` file to be used.

**For Docker Executor (`FAAS_EXECUTOR_TYPE="docker"`):**

- `DOCKER_HOST`: Specifies the Docker daemon socket/address (e.g., `unix:///var/run/docker.sock` or `tcp://localhost:2375`).

## macOS Development Setup for Firecracker

Firecracker is built on KVM and requires a Linux kernel. Therefore, to develop or test the Firecracker executor on macOS, you must use a Linux Virtual Machine (VM).

**Key Requirements for the Linux VM:**

- **Nested Virtualization:** Must be enabled if your VM is itself virtualized (e.g., VMware on top of macOS virtualization).
- **KVM:** The Linux kernel must have KVM modules enabled (`/dev/kvm` should exist).
- **Sufficient Resources:** Allocate enough CPU cores and RAM to the VM.

**Recommended VM Setup:**

1.  **Virtualization Software:**

    - **VMware Fusion (Recommended for performance):**
      - Create a new Linux VM (e.g., Ubuntu Server/Desktop).
      - Ensure "Enable hypervisor applications in this virtual machine" (or similar wording for nested virtualization) is checked in the VM's "Processors & Memory" settings.
    - **VirtualBox (Free):**
      - Create a new Linux VM.
      - Nested virtualization might be more complex to enable or less performant. Search for instructions specific to your VirtualBox version and macOS.
    - **Lima (Open Source):**
      - Provides Linux virtual machines on macOS with a focus on container runtimes.
      - Check Lima documentation for enabling KVM support: `lima sudo systemctl start libvirtd` might be relevant.

2.  **Inside the Linux VM:**
    - Install `qemu-kvm` and other virtualization utilities:
      ```bash
      sudo apt update
      sudo apt install qemu-kvm libvirt-daemon-system libvirt-clients bridge-utils cpu-checker
      ```
    - Verify KVM is enabled:
      ```bash
      kvm-ok
      # It should report: "KVM acceleration can be used"
      ```
    - Add your user to the `libvirt` and `kvm` groups:
      ```bash
      sudo adduser $(whoami) libvirt
      sudo adduser $(whoami) kvm
      # Log out and log back in or reboot the VM for group changes to take effect.
      ```
    - **Firecracker Binary & Kernel:**
      - Download the latest `firecracker` release binary for `x86_64` (or `aarch64` if your Mac and VM are ARM-based) from the [Firecracker releases page](https://github.com/firecracker-microvm/firecracker/releases). Place it in a directory in your `PATH` (e.g., `/usr/local/bin`) or note its location for `FAAS_FC_BINARY_PATH`.
      - Download a compatible uncompressed Linux kernel binary (e.g., `vmlinux.bin`). You can find pre-built kernels [here](https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_demo/x86_64/kernels/vmlinux.bin) (for x86_64) or build one. Note its path for `FAAS_FC_KERNEL_PATH`.
    - **Project Files:**
      - Clone your project repository into the Linux VM.
      - Build the `faas-bin` and `tools/firecracker-rootfs-builder/build_rootfs.sh` within the VM.
      - Set the environment variables (`FAAS_FC_BINARY_PATH`, `FAAS_FC_KERNEL_PATH`, `FAAS_FC_ROOTFS_PATH`) to point to the correct paths _within the Linux VM_.

You can then run `faas-bin` or `cargo test` for Firecracker-related tests directly inside this Linux VM environment. Port forwarding can be set up in your VM software if you need to access the `faas-gateway` from your macOS host.

## Testing

- Run unit and most integration tests: `cargo test`
- Many tests, especially those in `faas-tester` and executor-specific tests, might require environment variables to be set. It's highly recommended to create a `.env` file in the project root and populate it with the necessary configuration (see "Configuration" section). `dotenvy` is used by test setups to load these variables.
- **Firecracker Executor Tests:**
  - These tests are often marked with `#[ignore]` by default because they require a specific Firecracker setup (binary, kernel, rootfs, and KVM enabled).
  - To run them: `cargo test -- --ignored`
  - Ensure all `FAAS_FC_*` environment variables are correctly set.
  - **On macOS, these tests must be run inside the Linux VM environment described above.**
- Run executor-specific tests (require environment variables set and dependencies like Firecracker binary/kernel/rootfs available): `cargo test -- --ignored` (run within specific crates like `faas-tester` or workspace root).

## License

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
