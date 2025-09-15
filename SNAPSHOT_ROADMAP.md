# Snapshot & Branching Feature Roadmap

## Vision
Transform our FaaS system from fast container execution (150ms with warm pools) to a **computational branching platform** with sub-250ms VM snapshot/restore capabilities, similar to Morph Cloud's Infinibranch technology.

## Current State
- ✅ 3-4x performance improvement with warm container pools
- ✅ 122-150ms execution for simple commands
- ✅ Extensible environment registry
- ✅ Docker-based execution with volume caching
- ❌ No snapshot capability
- ❌ No branching/forking support
- ❌ No state preservation

## Target Capabilities
- Sub-250ms snapshot creation and restore
- Instant branching (10-50ms per branch)
- Process state preservation
- Memory deduplication
- Copy-on-write storage
- Parallel execution from checkpoints

## Technical Architecture Requirements

### Core Technologies Needed

#### 1. CRIU Integration (Checkpoint/Restore In Userspace)
- Linux kernel 3.11+ with CONFIG_CHECKPOINT_RESTORE
- CRIU 3.17+ for container checkpointing
- Integration with Docker/containerd for container snapshots

#### 2. Firecracker Snapshot Support
- Firecracker 1.4+ with snapshot API
- KVM acceleration
- Memory ballooning for efficient snapshots
- UFFD (userfaultfd) for lazy memory loading

#### 3. Storage Layer
- Copy-on-write filesystem (BTRFS/ZFS)
- NVMe storage for snapshot data
- Optional: Distributed storage (Ceph/MinIO) for snapshot sharing

#### 4. Memory Management
- KSM (Kernel Same-page Merging) for deduplication
- Hugepages support
- NUMA-aware memory allocation

## Development Phases

### Phase 1: Foundation (Weeks 1-3)
**Goal**: Basic snapshot infrastructure

#### Week 1: CRIU Integration
- [ ] Install and configure CRIU on development environment
- [ ] Create Rust bindings for CRIU API
- [ ] Implement basic container checkpoint/restore
- [ ] Test with simple Alpine containers

```rust
// crates/faas-executor/src/snapshot/criu.rs
pub struct CriuManager {
    socket_path: PathBuf,
    images_dir: PathBuf,
}

impl CriuManager {
    pub async fn checkpoint(&self, container_id: &str) -> Result<SnapshotId> {
        // Implement CRIU checkpoint
    }

    pub async fn restore(&self, snapshot_id: &str) -> Result<String> {
        // Implement CRIU restore
    }
}
```

#### Week 2: Firecracker Snapshot API
- [ ] Upgrade Firecracker integration to support snapshots
- [ ] Implement snapshot creation for microVMs
- [ ] Add restore functionality
- [ ] Benchmark snapshot/restore times

```rust
// crates/faas-executor/src/snapshot/firecracker.rs
pub struct FirecrackerSnapshotManager {
    firecracker_path: PathBuf,
    snapshots_dir: PathBuf,
}

impl FirecrackerSnapshotManager {
    pub async fn create_snapshot(&self, vm_id: &str) -> Result<Snapshot> {
        // Pause VM, snapshot memory and state
    }

    pub async fn restore_from_snapshot(&self, snapshot: &Snapshot) -> Result<String> {
        // Restore VM from snapshot
    }
}
```

#### Week 3: Storage Layer
- [ ] Set up BTRFS/ZFS for CoW snapshots
- [ ] Implement snapshot storage manager
- [ ] Add compression and deduplication
- [ ] Create cleanup policies

### Phase 2: Performance Optimization (Weeks 4-6)
**Goal**: Achieve sub-250ms snapshot/restore

#### Week 4: Memory Optimization
- [ ] Enable KSM for memory deduplication
- [ ] Implement lazy memory loading with UFFD
- [ ] Add memory pre-warming strategies
- [ ] Optimize page fault handling

#### Week 5: Storage Optimization
- [ ] Implement incremental snapshots
- [ ] Add snapshot caching layer
- [ ] Optimize I/O paths with io_uring
- [ ] Benchmark different storage backends

#### Week 6: Network Optimization
- [ ] Implement fast network namespace creation
- [ ] Add SR-IOV support for network acceleration
- [ ] Optimize NAT and routing setup
- [ ] Test with high network throughput workloads

### Phase 3: Branching Implementation (Weeks 7-9)
**Goal**: Enable instant VM forking

#### Week 7: Basic Branching
- [ ] Implement CoW overlay creation
- [ ] Add branch tracking and management
- [ ] Create parent-child relationship model
- [ ] Test with simple branching scenarios

```rust
// crates/faas-executor/src/snapshot/branch.rs
pub struct BranchManager {
    snapshots: Arc<RwLock<HashMap<String, Snapshot>>>,
    branches: Arc<RwLock<HashMap<String, Vec<Branch>>>>,
}

impl BranchManager {
    pub async fn create_branch(&self, parent_id: &str) -> Result<Branch> {
        // Create CoW overlay from parent
    }

    pub async fn fork(&self, parent_id: &str, count: usize) -> Result<Vec<Branch>> {
        // Create multiple branches in parallel
    }
}
```

#### Week 8: Parallel Execution
- [ ] Implement parallel branch orchestration
- [ ] Add resource isolation between branches
- [ ] Create branch synchronization primitives
- [ ] Test with parallel workloads

#### Week 9: Advanced Features
- [ ] Add branch merging capabilities
- [ ] Implement differential snapshots
- [ ] Create branch visualization tools
- [ ] Add time-travel debugging support

### Phase 4: SDK Integration (Weeks 10-12)
**Goal**: Developer-friendly APIs

#### Week 10: Core API Design
- [ ] Design snapshot/branch REST API
- [ ] Implement gRPC service for low-latency
- [ ] Add WebSocket support for real-time updates
- [ ] Create OpenAPI specification

#### Week 11: SDK Implementation
- [ ] Python SDK with snapshot support
- [ ] TypeScript SDK with branching
- [ ] Rust SDK for high-performance use cases
- [ ] CLI tool for snapshot management

#### Week 12: Testing & Documentation
- [ ] Comprehensive integration tests
- [ ] Performance benchmarks
- [ ] Security audit
- [ ] Developer documentation

## Performance Targets

### Snapshot Operations
| Operation | Current | Target | Stretch Goal |
|-----------|---------|--------|--------------|
| Container Start | 150ms | - | - |
| Snapshot Create | N/A | 250ms | 100ms |
| Snapshot Restore | N/A | 250ms | 150ms |
| Branch Create | N/A | 50ms | 10ms |
| Memory Dedup | N/A | 60% | 80% |

### Scalability Targets
- Support 100+ branches from single snapshot
- Handle 1000+ snapshots in registry
- Scale to 10,000+ concurrent executions

## Technical Challenges & Solutions

### Challenge 1: Memory Management
**Problem**: Large memory footprint for multiple VMs
**Solution**:
- KSM for deduplication
- UFFD for lazy loading
- Memory ballooning
- Swap to NVMe

### Challenge 2: Network Isolation
**Problem**: Creating network namespaces is slow
**Solution**:
- Pre-created network namespace pool
- eBPF for fast packet filtering
- SR-IOV for hardware acceleration

### Challenge 3: Storage Performance
**Problem**: Snapshot I/O can be bottleneck
**Solution**:
- NVMe storage with SPDK
- Distributed cache with RDMA
- Incremental snapshots
- Compression with zstd

### Challenge 4: Orchestration Complexity
**Problem**: Managing thousands of branches
**Solution**:
- Hierarchical branch tracking
- Garbage collection policies
- Resource quotas and limits
- Monitoring and observability

## Success Metrics

### Performance KPIs
- [ ] Achieve <250ms snapshot/restore
- [ ] Support 100+ parallel branches
- [ ] Maintain 99.9% reliability
- [ ] <10ms branch creation time

### Developer Experience KPIs
- [ ] SDK available in 3+ languages
- [ ] <5 minute time to first snapshot
- [ ] Comprehensive documentation
- [ ] Active community support

## Risk Mitigation

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| CRIU compatibility issues | Medium | High | Maintain Docker fallback |
| Performance targets not met | Low | High | Incremental optimization approach |
| Memory exhaustion with many branches | Medium | Medium | Implement strict resource limits |
| Network namespace creation bottleneck | Low | Medium | Pre-warm namespace pool |

## Alternative Approaches

### Option 1: Container-Only Snapshots
- Use Docker checkpoint/restore
- Simpler implementation
- Limited to container workloads
- ~500ms snapshot/restore

### Option 2: Full VM Snapshots
- Use QEMU/KVM snapshots
- Complete system state
- Higher overhead
- ~1-2s snapshot/restore

### Option 3: Hybrid Approach (Recommended)
- Containers for development workloads
- Firecracker for production isolation
- CRIU for container checkpoints
- Best performance/flexibility balance

## Next Steps

1. **Week 1**: Set up development environment with CRIU
2. **Week 1**: Create proof-of-concept snapshot implementation
3. **Week 2**: Benchmark against Morph Cloud performance
4. **Week 2**: Begin Firecracker snapshot integration
5. **Week 3**: Implement storage layer with CoW

## Resources Required

### Infrastructure
- Linux server with KVM support
- NVMe storage (1TB+)
- 64GB+ RAM for testing
- NVIDIA GPU (optional, for AI workloads)

### Software
- CRIU 3.17+
- Firecracker 1.4+
- Docker 24+ with checkpoint support
- BTRFS/ZFS filesystem
- Rust 1.75+ toolchain

### Team
- 2-3 engineers for core implementation
- 1 DevOps engineer for infrastructure
- 1 technical writer for documentation

## References

- [CRIU Documentation](https://criu.org/)
- [Firecracker Snapshots](https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/snapshot-support.md)
- [Morph Cloud Architecture](https://morph.so/blog/infinibranch/)
- [Linux KSM](https://www.kernel.org/doc/html/latest/admin-guide/mm/ksm.html)
- [UFFD (userfaultfd)](https://www.kernel.org/doc/html/latest/admin-guide/mm/userfaultfd.html)