# FaaS Platform on Tangle

## Overview

This project implements a Function-as-a-Service (FaaS) platform designed to run untrusted code securely within isolated environments, orchestrated via Tangle interactions (though primarily exposing an HTTP gateway currently). It leverages technologies like Firecracker for microVM isolation, providing a robust foundation for executing arbitrary functions triggered by external requests.

The platform is structured as a collection of Rust crates, enabling modular development and testing.

## Architecture & Packages

The repository is organized into the following key crates:

- **`faas-common`**: Contains shared data structures (e.g., `InvocationResult`, `InvokeRequest`), types, constants, and error definitions used across multiple crates.
- **`faas-guest-agent`**: A small binary designed to run _inside_ the execution environment (specifically Firecracker microVMs). It listens for invocation details (command, payload) via vsock, executes the requested command, captures its stdout/stderr, and sends the `InvocationResult` back to the host executor. It is statically compiled against musl libc for portability.
- **`faas-executor-firecracker`**: Implements the executor interface for running functions within Firecracker microVMs. It manages the lifecycle of microVMs (creation, starting, stopping, cleanup), interacts with the `faas-guest-agent` via vsock, and requires a pre-built Firecracker-compatible kernel and root filesystem containing the guest agent.
- **(Planned) `faas-executor-docker`**: (Future implementation) Will implement the executor interface for running functions within Docker containers.
- **`faas-orchestrator`**: The core component responsible for receiving function invocation requests. It selects the appropriate executor based on configuration or request details (currently defaults to Firecracker), manages the execution lifecycle via the chosen executor, handles timeouts/errors, and aggregates the results.
- **`faas-gateway`**: Provides the public-facing HTTP API (built with Axum) for invoking functions. It receives user requests, forwards them to the `faas-orchestrator`, and returns the results as JSON responses.
- **`faas-lib`**: Contains core library components, potentially shared logic used by the orchestrator or other parts of the system.
- **`faas-bin`**: The main executable binary. It initializes and runs the necessary services, likely including the `faas-gateway` and the `faas-orchestrator` with configured executors. (Note: While potentially deployable as a Tangle Blueprint, the primary user interaction model currently described is via the HTTP gateway).
- **`faas-tester`**: Holds integration tests and end-to-end tests for verifying the functionality of the gateway, orchestrator, and executors.
- **`tools/firecracker-rootfs-builder`**: Contains scripts and configuration (using Buildroot via Docker) to build the minimal `rootfs.ext4` filesystem image required by the `faas-executor-firecracker`.

## Getting Started

### Prerequisites

1.  **Rust Toolchain:** Install Rust (latest stable recommended) via `rustup`.
2.  **Docker:** Required for building the Firecracker rootfs using the provided script.
3.  **(For Firecracker Executor):**
    - `firecracker` binary accessible in your `PATH`.
    - Firecracker-compatible kernel (`vmlinux.bin`).
    - Build the root filesystem (see below).

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

1.  **Set Environment Variables:** Configure the paths required by the Firecracker executor:

    ```bash
    # Path to the Firecracker binary
    export FC_BINARY_PATH="/path/to/your/firecracker"
    # Path to the compatible Linux kernel
    export FC_KERNEL_PATH="/path/to/your/vmlinux.bin"
    # Path to the rootfs built by the script
    export FC_ROOTFS_PATH="/path/to/your/faas/tools/firecracker-rootfs-builder/output/rootfs.ext4"

    # Optional: Configure gateway port, logging, etc.
    # export FAAS_GATEWAY_PORT=8080
    # export RUST_LOG=info
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
- **Purpose:** Executes a command within an isolated environment. The `{function_id}` path parameter is currently used for logging/tracing but does not directly select pre-registered functions. The execution details are provided entirely within the request body.
- **Request Body (JSON):**

  ```json
  {
    "image": "ignored-for-firecracker", // Currently unused by Firecracker, might be used for Docker later
    "command": ["/path/to/executable", "arg1", "arg2"], // Command and arguments to run inside the guest
    "env_vars": ["VAR1=value1", "VAR2=value2"], // Optional: Environment variables for the command
    "payload": [
      /* array of bytes (numbers 0-255) */
    ] // Raw bytes to be piped to the command's stdin
  }
  ```

  _Note: Clients sending JSON should represent the raw `payload` bytes appropriately, e.g., as an array of numbers._

- **Success Response (200 OK, JSON):** Returns the execution result.

  ```json
  {
    "stdout": "Output written to stdout by the command...",
    "stderr": "Output written to stderr by the command...",
    "error": null // Null if the command executed without internal errors reported by the agent
  }
  ```

- **Execution Error Response (200 OK, JSON):** If the command ran but the _guest agent_ reported an execution failure (e.g., command not found, non-zero exit code interpreted as error).

  ```json
  {
    "stdout": "...",
    "stderr": "...",
    "error": "Error message reported by guest agent or executor"
  }
  ```

- **Gateway/Orchestrator Error Response (4xx/5xx, JSON):** If there's an issue processing the request _before or during_ orchestration (e.g., bad request format, orchestrator failure, timeout).
  ```json
  {
    "error": "Specific error message (e.g., Bad Request: ..., Internal server error)"
  }
  ```

### Job Execution Flow (Firecracker)

1.  Client sends `POST /functions/{id}/invoke` request to `faas-gateway`.
2.  Gateway parses the request and calls `Orchestrator::schedule_execution`.
3.  Orchestrator validates the request and selects the Firecracker executor.
4.  `faas-executor-firecracker` sets up a new Firecracker microVM using the configured kernel (`FC_KERNEL_PATH`) and rootfs (`FC_ROOTFS_PATH`).
5.  The microVM boots, and the `/init` script inside the rootfs starts the `faas-guest-agent`.
6.  The guest agent establishes communication with the host executor via vsock.
7.  The executor sends the `command`, `env_vars`, and `payload` to the guest agent via vsock.
8.  The guest agent sets up the environment, executes the command, pipes the payload to its stdin, and captures its stdout and stderr.
9.  Upon command completion, the guest agent packages the `stdout`, `stderr`, and any execution errors into an `InvocationResult` and sends it back to the host executor via vsock.
10. The executor receives the result, signals the microVM to shut down, and cleans up resources.
11. The executor returns the `InvocationResult` to the orchestrator.
12. The orchestrator returns the result to the gateway.
13. The gateway serializes the `InvocationResult` to JSON and sends the HTTP response to the client.

## Configuration

Key environment variables:

- `FC_BINARY_PATH`: Absolute path to the `firecracker` executable.
- `FC_KERNEL_PATH`: Absolute path to the `vmlinux.bin` kernel file.
- `FC_ROOTFS_PATH`: Absolute path to the `rootfs.ext4` file.
- `RUST_LOG`: Controls logging level (e.g., `info`, `debug`, `faas_gateway=debug,warn`).
- `FAAS_GATEWAY_PORT`: Port for the HTTP gateway (default depends on implementation).

## Testing

- Run unit and integration tests: `cargo test`
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
