# CI/CD Pipeline Example

Production-ready CI/CD pipeline implementation using FaaS execution modes and parallel testing.

## Prerequisites

- **Docker** - Required for container execution
  - Download from: https://www.docker.com/products/docker-desktop
- **FaaS Gateway Server** - Must be running on port 8080
- **Docker Images** - The following images will be pulled automatically:
  - `alpine:latest`
  - `node:18-alpine`
  - `python:3.11-alpine`
  - `rust:latest` (if running full Rust examples)

## Features

- ✅ Multi-stage pipeline (build, test, security, deploy)
- ✅ Parallel test execution
- ✅ Security scanning integration
- ✅ Docker image building
- ✅ Execution modes (cached, fresh)
- ✅ Artifact management
- ✅ Real YAML configuration parsing
- ✅ Timeout handling per stage

## Running

```bash
# 1. Start FaaS gateway server (in one terminal)
cargo run --release --package faas-gateway-server

# 2. Wait for gateway to start (look for "listening on 0.0.0.0:8080")

# 3. Run the CI/CD pipeline (in another terminal)
cargo run --release --package cicd
```

## Note

First run may take several minutes as Docker images are pulled. Node.js and Python images are relatively small (~50-100MB), but Rust images can be large (1GB+).

## Pipeline Stages

1. **Build Stage**: Compile application
2. **Test Stage**: Unit tests, integration tests, e2e tests (parallel)
3. **Security Stage**: Dependency audit, SAST scanning
4. **Deploy Stage**: Build and push Docker image

## Use Cases

- Automated testing pipelines
- Build verification
- Security compliance checks
- Multi-environment deployments

## Lines of Code

357 lines - Complete CI/CD workflow
