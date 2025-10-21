# Repository Guidelines

## Project Structure & Module Organization
The Rust workspace in `Cargo.toml` spans `crates/` (gateway, executor, guest agent, usage tracker), `faas-lib/` and `faas-bin/` (blueprint logic and on-chain operator tooling), plus `faas-tester/` and top-level `tests/` for integration suites. Smart-contract blueprints live in `contracts/` (Foundry), while language SDKs sit under `sdks/`—the TypeScript client is in `sdks/typescript`. Examples and benchmarks are in `examples/`, automation scripts in `scripts/`, and reference material within `docs/`, `resources/`, and `monitoring/`.

## Build, Test, and Development Commands
-- `cargo build --workspace --release` builds all Rust crates.
- `cargo run --release --package faas-gateway-server` starts the HTTP gateway; `scripts/start-gateway.sh` wraps env vars.
- `cargo test --workspace` runs unit suites; add `-p faas-tester` for orchestration-level checks.
- `forge test` (inside `contracts/`) validates solidity blueprints with Foundry.
- `npm install && npm run build` (inside `sdks/typescript`) compiles the TypeScript SDK; `npm test` executes the Jest suite.
- `scripts/setup_dev.sh` provisions rustup, Docker/Firecracker prerequisites, and runs smoke tests; rerun after major toolchain updates.

## Coding Style & Naming Conventions
Rust code follows `cargo fmt` (nightly toolchain) and `cargo clippy --workspace --all-targets` with warnings treated seriously; prefer snake_case modules, CamelCase types, and descriptive errors via `thiserror/anyhow`. TypeScript sources in `sdks/typescript` rely on 2-space indentation, ESLint defaults, and exported classes/interfaces in PascalCase; compiled output stays in `dist/`. Format TOML manifests with `taplo fmt`. Avoid committing generated artifacts outside `dist/` or `out/` directories already tracked.

## Testing Guidelines
Target green `cargo test --workspace` before pushing; integration tests that hit Docker or Firecracker are marked `ignored`—run with `cargo test -- --ignored` prior to release branches. Coverage reports come from `cargo tarpaulin` and performance checks from `cargo criterion` (both installed via `scripts/setup_dev.sh`). When touching SDKs, run the corresponding `npm test` or language-specific harness. Foundry tests must pass without forking mainnet; reset caches via `forge clean` if artifacts in `contracts/out/` cause drift. Document nontrivial test scenarios in PR descriptions.

## Commit & Pull Request Guidelines
Follow conventional commits (`feat:`, `fix:`, `chore:`, etc.) as used in recent history (`feat: complete WebSocket…`, `fix(examples): resolve…`). Keep commits scoped and readable; squash only when intermediate history adds no value. Pull requests should include: concise summary, linked issues, runtime/test evidence (commands or CI links), and any relevant screenshots or logs for operator tooling. Flag breaking changes or new infra requirements in bold near the top. Request reviews from owners of touched areas (`crates/`, `contracts/`, `sdks/`) and ensure CI signals are green before merging.
