# GitHub Actions Workflows

## CI Workflow (`ci.yml`)

Runs on every push to `main`/`develop` and on pull requests.

### Jobs

- **test**: Rust tests on Ubuntu and macOS (stable + nightly)
- **firecracker**: Firecracker microVM tests with KVM
- **integration**: Docker integration tests with gateway
- **typescript-sdk**: Build and test TypeScript SDK (Node 18, 20)
- **python-sdk**: Build and test Python SDK (Python 3.8-3.12)
- **security**: Cargo audit for vulnerability scanning
- **examples**: Build all example packages

## SDK Publishing Workflows

### TypeScript SDK (`publish-typescript-sdk.yml`)

Automatically publishes to npm when changes are pushed to `sdks/typescript/`.

**Required secrets:**
- `NPM_TOKEN`: npm access token with publish permissions

**Process:**
1. Uses release-please for automatic versioning
2. Creates release PR with version bump and CHANGELOG
3. When PR is merged, publishes to npm
4. Creates GitHub release

### Python SDK (`publish-python-sdk.yml`)

Automatically publishes to PyPI when changes are pushed to `sdks/python/`.

**Required secrets:**
- `PYPI_TOKEN`: PyPI API token with upload permissions

**Process:**
1. Uses release-please for automatic versioning
2. Creates release PR with version bump and CHANGELOG
3. When PR is merged, publishes to PyPI
4. Creates GitHub release

## Binary Release Workflow (`release.yml`)

Uses cargo-dist for automated binary releases.

Triggered by pushing version tags: `v1.0.0`, `v0.1.0-prerelease`, etc.

## Foundry Workflow (`foundry.yml`)

Tests Solidity smart contracts using Foundry.

## Setup Instructions

### Required Repository Secrets

1. Navigate to repository Settings → Secrets and variables → Actions
2. Add the following secrets:

```
NPM_TOKEN        - For publishing @faas-platform/sdk to npm
PYPI_TOKEN       - For publishing faas-sdk to PyPI
```

### Obtaining Tokens

**NPM Token:**
```bash
npm login
npm token create --type=automation
```

**PyPI Token:**
1. Visit https://pypi.org/manage/account/token/
2. Create new API token
3. Scope: Entire account or specific project

### Version Management

Both SDK publishing workflows use [release-please](https://github.com/googleapis/release-please) for automated semantic versioning based on conventional commits:

- `feat:` → minor version bump
- `fix:` → patch version bump
- `feat!:` or `BREAKING CHANGE:` → major version bump

Release-please automatically:
- Creates release PRs with version bumps
- Updates CHANGELOG.md
- Creates GitHub releases
- Triggers npm/PyPI publishing

## Manual Publishing

If automatic publishing fails, you can publish manually:

**TypeScript:**
```bash
cd sdks/typescript
npm version patch  # or minor, major
npm publish
```

**Python:**
```bash
cd sdks/python
python -m build
twine upload dist/*
```
