# Migration Guide

> [!WARNING]
> This repository is **deprecated**. Do not start new work here.

This repository is being phased out in favor of the `sandbox-runtime` layer plus product-specific blueprints.

## What To Do Right Now

- **New runtime/executor features** → contribute to `sandbox-runtime` (see target repos below).
- **New product/business logic** → contribute to the relevant product blueprint repository.
- **Bug fixes for existing deployments** → critical fixes only; open a PR here and tag it `maintenance`.
- **Everything else** → do not send to this repo.

## Target Repositories

| Repository | Purpose | URL |
| --- | --- | --- |
| `sandbox-runtime` | Runtime core, execution primitives, storage/cache, guest agent, telemetry | *(Not yet public — URL will be added here when the repo is published)* |
| Product blueprint repos | Gateway/API adapters, on-chain operator logic, contracts, SDKs, integration tests | *(Not yet created — URLs will be added here as each product repo is established)* |

> **Maintainer note:** when target repo URLs become available, update this table and the corresponding entries in [README.md](README.md) and [docs/STATUS.md](docs/STATUS.md).

## Capability Mapping

| Legacy module/capability (this repo) | Target layer/repo | Status | Notes for consumers |
| --- | --- | --- | --- |
| `crates/faas-executor` (Docker/Firecracker execution) | `sandbox-runtime` runtime core | In progress | Move new runtime features and fixes to `sandbox-runtime`; keep this crate in maintenance-only mode. |
| Execution modes (ephemeral/cached/snapshot/fork/prewarmed) | `sandbox-runtime` execution primitives | In progress | Treat this repo as compatibility reference only. |
| Storage, artifact, and snapshot handling | `sandbox-runtime` storage/cache layer | Planned | New persistence behavior should land in `sandbox-runtime`; avoid adding new storage features here. |
| `crates/faas-gateway` + `crates/faas-gateway-server` HTTP API | Product blueprint repos (API adapters) + `sandbox-runtime` APIs | Planned | Client-facing APIs should be owned by product blueprints that compose `sandbox-runtime`. |
| `faas-lib` + `faas-bin` on-chain operator blueprint logic | Product blueprint repos | Planned | New chain/business workflows should be implemented in product blueprints, not this repo. |
| `contracts/` smart-contract blueprints | Product blueprint repos (`contracts/`) | Planned | Keep contract changes tied to product blueprint releases. |
| SDK surfaces in `crates/faas-sdk` and `sdks/typescript` | Product blueprint SDK repos (runtime client wrappers) | Planned | New SDK APIs should track product blueprint contracts/API, with `sandbox-runtime` as backend layer. |
| `crates/faas-guest-agent` and `crates/faas-usage-tracker` | `sandbox-runtime` agent/telemetry components | Planned | Migrate telemetry and guest-agent internals before adding metrics/billing features. |
| `faas-tester`, `examples/`, top-level `tests/` | Product blueprint integration test suites | Planned | Shift end-to-end test ownership to each product blueprint repo. |

## Migration Policy

1. Do not introduce net-new runtime behavior in this repository.
2. Only accept critical fixes required to keep existing deployments stable during migration.
3. Land all new feature work in `sandbox-runtime` and/or product blueprint repositories.
4. Update downstream consumers to depend on product blueprint SDKs and APIs rather than this repo directly.

