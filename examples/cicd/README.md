# CI/CD Pipeline Example

Production-ready CI/CD pipeline implementation using FaaS execution modes and parallel testing.

## Features

- ✅ Multi-stage pipeline (build, test, security, deploy)
- ✅ Parallel test execution
- ✅ Security scanning integration
- ✅ Docker image building
- ✅ Execution modes (cached, fresh)
- ✅ Artifact management

## Running

```bash
# Start FaaS gateway server
cargo run --release --package faas-gateway-server

# Run the CI/CD pipeline
cargo run --release --package cicd
```

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
