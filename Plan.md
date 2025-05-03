# FaaS-Blueprint Implementation Plan (Revised)

**Project Lead:** Drew

**Goal:** Build a production-grade, secure, multi-language FaaS system integrated into the Tangle Blueprint framework, based on the architecture defined in `FaaS.md`.

**Strategy:** Adopt a phased, iterative approach focusing on delivering testable increments. Each phase builds upon the last, prioritizing foundation stability, core Blueprint integration, secure sandboxing (TDX/Firecker), advanced features, and continuous testing.

---

## Phase 0: Foundation Cleanup & API Definition (Completed)

- **Goal:** Stabilize existing library interfaces, particularly `faas-gateway` API. Ensure consistency and remove dead code.
- **Tasks:**
  - [x] Analyze `faas-gateway` API types (`Api...` vs. `InvokeRequest`).
  - [x] Decide on the definitive API structure for initial Blueprint integration (using `InvokeRequest` model).
  - [x] Refactor `faas-gateway` to remove unused API types.
  - [x] Verify `faas-tester` integration tests align with the cleaned-up gateway API.
  - [x] Update `Plan.md` with this revised plan structure.
  - [ ] Update `FaaS.md` if needed to clarify the _initial_ API surface vs. the _eventual_ full API. (Deferred - review after Phase 1)

## Phase 1: Core Blueprint Integration (Completed)

- **Goal:** Implement the basic FaaS execution flow as a Tangle Blueprint service using the existing Docker-based executor.
- **Tasks:**
  - [x] Define Tangle Job (`ExecuteFunction`) structure in `faas-common`. (`ExecuteFunctionArgs`)
  - [x] Implement `faas-blueprint-lib` (`faas-lib`):
    - [x] Create `FaaSContext`.
    - [x] Implement `execute_function_job` handler.
  - [x] Implement `faas-bin`:
    - [x] Set up `BlueprintRunner` with `FaaSContext`.
    - [x] Route the `ExecuteFunction` job ID to the handler.
    - [x] Verify/Update basic Dockerfile for the `faas-bin` service.
  - [x] Add `TangleTestHarness` tests in `faas-bin`.
  - [ ] Refine mapping of `InvocationResult` fields (stdout, stderr, error) to `Vec<OutputValue>` in `faas-lib/src/jobs.rs`.

## Phase 2: Secure Sandboxing (Firecracker Integration - MVP)

- **Goal:** Replace the Docker executor with Firecracker for enhanced isolation.
- **Tasks:**
  - [x] Refactor executor interface (`SandboxExecutor` trait).
  - [x] Update `DockerExecutor` to implement `SandboxExecutor`.
  - [x] Update `faas-orchestrator` to use `SandboxExecutor`.
  - [x] Implement `FirecrackerExecutor` async structure (using `_rt-tokio`).
  - [ ] **Refine `FirecrackerExecutor` implementation:**
    - [ ] Implement instance monitoring & result retrieval (replace sleep/timeout with robust VMM/process check).
    - [ ] Implement robust cleanup (RAII guard for all resources).
  - [ ] **Acquire/Build Base Rootfs & Kernel:**
    - [ ] **Use Buildroot to create minimal rootfs:** Configure Buildroot for musl, minimal packages, target architecture. (NEW)
    - [ ] Obtain or build a compatible Linux kernel binary (`vmlinux.bin`).
  - [ ] **Implement Guest Agent Build & Integration:**
    - [x] Build `faas-guest-agent` statically using `x86_64-unknown-linux-musl` target.
    - [ ] Develop script/process or integrate into Buildroot to inject the agent into the rootfs at `/app/faas-guest-agent`.
    - [ ] Ensure the Buildroot init system starts the agent on boot.
  - [ ] Implement vsock communication (Host <-> Guest Agent).
    - [x] Host-side executor logic.
    - [x] Guest-side agent logic.
    - [ ] Refine Guest Agent Execution.
    - [ ] Refine Host-Guest Error Handling.
  - [ ] Implement rootfs preparation selection logic in `FirecrackerExecutor` (use path from `SandboxConfig.source` which points to the final Buildroot image).
  - [ ] Add configuration loading (Paths, VM resources, Executor type selection).
    - [x] Executor type selection in `FaaSContext`.
    - [ ] Load paths/resources from `BlueprintEnvironment`.
  - [ ] Add integration tests in `faas-tester` for `FirecrackerExecutor`.
  - [ ] Update `TangleTestHarness` tests in `faas-bin` for `FirecrackerExecutor`.

## Phase 3: TDX Integration & Attestation

- **Goal:** Integrate Intel TDX for hardware-level memory protection and add attestation capabilities.
- **Tasks:**
  - [ ] Modify `FirecrackerExecutor` to launch VMs as TDX Trust Domains (TDs).
  - [ ] Implement mechanisms within the guest TD to generate TDX quotes (attestation reports).
  - [ ] Implement logic (likely in `faas-orchestrator` or `faas-blueprint-lib` context) to request and verify TDX quotes from workers before scheduling sensitive jobs.
  - [ ] Update infrastructure requirements (`check_env.sh`) and documentation for TDX setup.
  - [ ] Add tests verifying TDX execution and attestation flow (may require specific hardware or simulation).

## Phase 4: Dependency Management & Multi-Language Support

- **Goal:** Implement robust dependency handling and create base runtimes.
- **Tasks:**
  - [ ] Implement `Orchestrator::register_function` logic:
    - [ ] Define how function code/source is provided (e.g., fetch from IPFS/URL specified in `FunctionDefinition`?).
    - [ ] Potentially trigger pre-build/cache steps upon registration.
  - [ ] Design dependency specification format (e.g., enhancing `FunctionDefinition`).
  - [ ] Implement dependency resolution logic (parsing `package.json`, `requirements.txt`).
  - [ ] Implement dependency caching mechanism.
  - [ ] Create minimal, secure base images/rootfs for Node.js and Python suitable for Firecracker/TDX.
  - [ ] Integrate dependency installation into the sandbox preparation step.
  - [ ] Add tests for different languages and dependency scenarios.

## Phase 5: Advanced Orchestration & Performance

- **Goal:** Enhance scalability, reliability, and performance.
- **Tasks:**
  - [ ] Implement multi-worker scheduling logic (if applicable beyond Blueprint's inherent distribution).
  - [ ] Implement warm container/microVM management (pooling, reuse) within executor/orchestrator logic.
  - [ ] Implement resource limits (CPU, memory) enforcement (via cgroups/Firecracker config).
  - [ ] Implement basic monitoring and logging export from functions/sandboxes.
  - [ ] Explore performance optimizations (e.g., snapshotting, result caching - optional).
  - [ ] Add load testing scenarios.

## Phase 6: Production Hardening & Final Touches

- **Goal:** Add final layers of security, monitoring, and documentation for production readiness.
- **Tasks:**
  - [ ] Implement fine-grained container security (seccomp, capabilities, non-root) within the sandbox.
  - [ ] Implement network policies for sandboxes.
  - [ ] Integrate vulnerability scanning for base images/dependencies.
  - [ ] Set up production-grade monitoring and alerting infrastructure.
  - [ ] Conduct security review/audit.
  - [ ] Finalize all documentation (`README`, `FaaS.md`, usage guides).
  - [ ] Comprehensive End-to-End testing across all features.

---

This revised plan provides a clearer, iterative path towards the production-grade system.
