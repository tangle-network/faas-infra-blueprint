# Status Snapshot

Snapshot date: March 2, 2026

## Current Health

- Repository status: deprecated, maintenance-only.
- Runtime and orchestration features are in migration planning/execution toward `sandbox-runtime` + product blueprint repos.
- Existing code remains buildable as a legacy reference stack, but ownership for new feature delivery is moving out of this repository.
- Documentation drift exists (for example, historical references to `docs/*` and `contracts/foundry.toml` paths that are no longer canonical).

## Outdated Dependency and Toolchain Risks

- `Cargo.toml` workspace `rust-version = "1.75"` can lag current compiler/security fixes and may diverge from the active tnt-core/blueprint toolchain.
- `scripts/setup_dev.sh` forces `rustup default nightly`, which can reduce reproducibility across contributors and CI.
- Git-sourced dependencies are not pinned to immutable commits (`blueprint-sdk`, `blueprint-client-tangle`, `docktopus` branch), increasing supply-chain and reproducibility risk.
- `sdks/typescript/package.json` allows `node >=16.0.0`; Node 16 reached end-of-life on September 11, 2023, so this baseline is no longer a safe default.
- SDK dependency ranges and runtime crates may drift from product blueprint expectations while migration is in progress.

## Immediate Steps (Safe Migration Track)

1. Freeze net-new features in this repo and treat all changes as compatibility or critical-fix only.
2. Establish canonical target repos for `sandbox-runtime` and each product blueprint, then link them from `README.md` and `MIGRATION.md`. *(Placeholder rows added to [MIGRATION.md](../MIGRATION.md#target-repositories) — update with real URLs when repos are public.)*
3. Pin git dependencies to explicit revisions during the transition window to improve reproducibility.
4. Align CI/dev toolchains with target-stack versions (Rust toolchain policy, Foundry config path, Node baseline).
5. Migrate one capability slice at a time (runtime core, gateway/API, SDK, contracts, telemetry), marking each row in `MIGRATION.md` as `Migrated` when complete.
6. After downstream consumers are moved, archive this repository or keep it as read-only legacy reference.

