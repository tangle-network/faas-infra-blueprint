# FaaS Platform Production Deployment Guide

## Executive Summary

This guide covers deploying the FaaS (Function-as-a-Service) platform to production environments. The platform supports multiple isolation technologies (Docker, Firecracker, CRIU) with automatic fallback mechanisms.

## Architecture Overview

### Core Components
- **faas-executor**: Main execution engine with pluggable isolation backends
- **faas-blueprint-lib**: SDK for function development
- **faas-common**: Shared types and utilities

### Isolation Technologies
1. **Docker** (Primary on macOS/Development)
2. **Firecracker** (Production Linux with KVM)
3. **CRIU** (Checkpoint/Restore for fast cold starts)

## Deployment Requirements

### Hardware Requirements
- **Minimum**: 4 CPU cores, 8GB RAM
- **Recommended**: 16+ CPU cores, 32GB+ RAM
- **Storage**: 100GB+ SSD for container images and snapshots
- **Network**: 10Gbps for production workloads

### Software Requirements
- **OS**: Linux kernel 5.10+ (Ubuntu 22.04 LTS recommended)
- **Container Runtime**: Docker 20.10+
- **For Firecracker**: KVM-enabled kernel
- **For CRIU**: Kernel with checkpoint/restore support

## Deployment Configurations

### 1. Development Environment (macOS/Windows)
```bash
# Uses Docker-only mode with stub implementations
./test-faas-platform test
```

### 2. Staging Environment (Linux VM)
```yaml
# docker-compose.yml
version: '3.8'
services:
  faas-executor:
    image: faas:latest
    privileged: true
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - ./functions:/functions
    environment:
      - FAAS_MODE=hybrid
      - ENABLE_CRIU=false
      - ENABLE_FIRECRACKER=false
```

### 3. Production Environment (Bare Metal Linux)
```yaml
# kubernetes/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: faas-executor
spec:
  replicas: 3
  template:
    spec:
      nodeSelector:
        faas.io/kvm: "true"
      containers:
      - name: faas-executor
        image: faas:latest
        securityContext:
          privileged: true
        resources:
          requests:
            memory: "4Gi"
            cpu: "2"
          limits:
            memory: "16Gi"
            cpu: "8"
        env:
        - name: FAAS_MODE
          value: "production"
        - name: ENABLE_FIRECRACKER
          value: "true"
        - name: ENABLE_CRIU
          value: "true"
```

## Performance Tuning

### Container Pool Configuration
```rust
// Recommended production settings
ContainerPoolConfig {
    max_idle: 50,
    min_idle: 10,
    max_total: 200,
    idle_timeout: Duration::from_secs(300),
    pre_warm: true,
}
```

### Cache Settings
```rust
CacheConfig {
    max_memory_mb: 8192,
    ttl_seconds: 3600,
    compression: CompressionType::LZ4,
}
```

### Firecracker VM Pools (Linux Production)
```rust
VmConfig {
    vcpu_count: 2,
    memory_size_mib: 512,
    kernel_path: "/var/lib/faas/kernel/vmlinux.bin",
    rootfs_path: "/var/lib/firecracker/rootfs/ubuntu.ext4",
    enable_networking: true,
}
```

## Monitoring & Observability

### Key Metrics
- **Function Execution Latency**: p50, p95, p99
- **Cold Start Duration**: By isolation type
- **Container Pool Utilization**: Hit rate, evictions
- **Resource Usage**: CPU, Memory, Disk I/O per function

### Prometheus Metrics
```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'faas-executor'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: /metrics
```

### Logging
```bash
# Enable structured logging
export RUST_LOG=faas_executor=info,faas_blueprint_lib=debug
export FAAS_LOG_FORMAT=json
```

## Security Considerations

### Network Isolation
- Use bridge networks for container isolation
- Enable firewall rules for Firecracker TAP interfaces
- Implement rate limiting at ingress

### Resource Limits
```rust
SandboxConfig {
    memory_limit: Some(512 * 1024 * 1024), // 512MB
    cpu_limit: Some(1.0), // 1 CPU core
    timeout: Duration::from_secs(30),
    network_enabled: false, // Disable for untrusted code
}
```

### Secret Management
- Use environment variables for runtime secrets
- Mount secrets as read-only volumes
- Rotate credentials regularly

## Deployment Checklist

### Pre-Deployment
- [ ] Verify kernel version and capabilities
- [ ] Install Docker and configure daemon
- [ ] Set up monitoring infrastructure
- [ ] Configure log aggregation
- [ ] Review security policies

### Deployment
- [ ] Deploy executor service
- [ ] Verify health endpoints
- [ ] Test isolation mechanisms
- [ ] Run smoke tests
- [ ] Enable auto-scaling

### Post-Deployment
- [ ] Monitor metrics dashboard
- [ ] Set up alerts
- [ ] Document runbooks
- [ ] Schedule performance reviews
- [ ] Plan capacity scaling

## Rollback Procedures

### Blue-Green Deployment
```bash
# Deploy new version
kubectl apply -f deployment-v2.yaml

# Verify health
curl http://faas-v2/health

# Switch traffic
kubectl patch service faas -p '{"spec":{"selector":{"version":"v2"}}}'

# Rollback if needed
kubectl patch service faas -p '{"spec":{"selector":{"version":"v1"}}}'
```

## Troubleshooting

### Common Issues

#### 1. Firecracker Not Starting
```bash
# Check KVM availability
ls -la /dev/kvm
# Output should show: crw-rw---- 1 root kvm ...

# Verify kernel modules
lsmod | grep kvm
```

#### 2. CRIU Checkpoint Failures
```bash
# Check CRIU capabilities
criu check --all

# Verify kernel config
cat /proc/config.gz | gunzip | grep CONFIG_CHECKPOINT
```

#### 3. Container Pool Exhaustion
```bash
# Increase pool limits
export FAAS_CONTAINER_POOL_MAX=500
export FAAS_CONTAINER_POOL_IDLE=100
```

## Support

- **Documentation**: [Internal Wiki]
- **Slack Channel**: #faas-platform
- **On-Call**: PagerDuty rotation
- **Escalation**: Platform Team Lead

## Version History

- **v0.1.0**: Initial release with Docker support
- **v0.2.0**: Added Firecracker integration
- **v0.3.0**: CRIU checkpoint/restore support
- **v0.4.0**: Production hardening and monitoring

---

Last Updated: 2024-01-20
Maintained by: Platform Engineering Team