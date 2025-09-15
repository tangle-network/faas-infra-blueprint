# Execution Platform

## Core Modes

```rust
pub enum Mode {
    Ephemeral,     // Destroy after execution
    Cached,        // Reuse warm container
    Checkpointed,  // Save/restore state
    Branched,      // Fork execution
    Persistent,    // Long-running
}
```

## Architecture

```rust
pub struct Executor {
    backends: Backends,
    memory: MemoryPool,
    snapshots: SnapshotStore,
    forks: ForkManager,
    cache: Cache,
}

struct Backends {
    container: ContainerExecutor,  // Docker
    vm: VmExecutor,                // Firecracker
}
```

## Performance Targets

| Operation | Target |
|-----------|--------|
| Simple function | <50ms |
| Cached execution | <150ms |
| Checkpoint | <200ms |
| Restore | <250ms |
| Fork | <50ms |

## Implementation

All execution flows through single entry point with mode selection:

```rust
impl Executor {
    pub async fn run(&self, req: Request) -> Result<Response> {
        match req.mode {
            Mode::Ephemeral => self.run_ephemeral(req).await,
            Mode::Cached => self.run_cached(req).await,
            Mode::Checkpointed => self.run_checkpointed(req).await,
            Mode::Branched => self.run_branched(req).await,
            Mode::Persistent => self.run_persistent(req).await,
        }
    }
}
```