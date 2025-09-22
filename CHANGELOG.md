# Changelog

## [Unreleased]

### Added
- Consolidated test runner `test-faas-platform` replacing multiple bash scripts
- Platform capability detection tests
- Comprehensive FaaS platform integration tests
- Docker executor with stdin support fix

### Changed
- Updated README.md to reflect actual project structure
- Cleaned up test infrastructure to use Rust's native test framework
- Fixed stdin handling in Docker executor (added `stdin_once: Some(true)`)
- Updated CRIU implementation for v3.19+ compatibility (removed `--ms` flag)

### Removed
- Redundant test scripts: `test-faas`, `test-faas-arm-criu`, `test-faas-x86`
- Temporary debug files: `test_stdin_debug.rs`, `stdin_test_simple.rs`
- Old deployment scripts: `test-blueprint-deployment.sh`, `test-real-faas-deployment.sh`
- Stale test documentation files
- Test Dockerfiles (6 removed): `.faas-test-x86.dockerfile` and all test variants
- Docker Compose test files: `docker-compose.full-test.yml`, `docker-compose.cached-test.yml`
- Build script: `build-cached-tests.sh`

### Fixed
- Docker executor stdin hanging issue
- All Docker integration tests now passing
- Comprehensive tests timing tolerances adjusted for concurrent execution
- Environment variables type mismatch in tests

## Platform Status

### Working
- ✅ Docker executor on all platforms
- ✅ All integration tests passing
- ✅ Comprehensive test suite

### Ready (Linux + KVM required)
- Firecracker microVM support
- CRIU checkpoint/restore
- Sub-100ms cold starts

### Known Limitations
- macOS: Docker-only execution (no KVM for Firecracker)
- Firecracker requires Linux with KVM enabled
- CRIU requires Linux kernel with checkpoint support