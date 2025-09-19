# Critical Tests Required for Production

## Priority 1: Core Executor Tests
```rust
// faas-executor/tests/docker_executor_test.rs
#[tokio::test]
async fn test_docker_execution_success() { }
#[tokio::test]
async fn test_docker_execution_timeout() { }
#[tokio::test]
async fn test_docker_resource_limits() { }
#[tokio::test]
async fn test_docker_cleanup_on_failure() { }
```

## Priority 2: Snapshot/Branch Tests
```rust
// faas-executor/tests/snapshot_test.rs
#[tokio::test]
async fn test_create_snapshot_running_container() { }
#[tokio::test]
async fn test_restore_snapshot_with_state() { }
#[tokio::test]
async fn test_branch_from_snapshot() { }
#[tokio::test]
async fn test_merge_branches_conflict_resolution() { }
```

## Priority 3: Job Integration Tests
```rust
// faas-lib/tests/job_integration.rs
#[tokio::test]
async fn test_execute_function_e2e() { }
#[tokio::test]
async fn test_execute_advanced_with_checkpoint() { }
#[tokio::test]
async fn test_instance_lifecycle_start_stop_pause() { }
```

## Priority 4: Edge Cases
```rust
// faas-lib/tests/edge_cases.rs
#[tokio::test]
async fn test_concurrent_executions_resource_contention() { }
#[tokio::test]
async fn test_oom_killer_behavior() { }
#[tokio::test]
async fn test_network_failure_during_execution() { }
#[tokio::test]
async fn test_disk_full_during_snapshot() { }
```

## Priority 5: Performance Tests
```rust
// faas-lib/tests/performance.rs
#[tokio::test]
async fn test_cold_start_under_100ms() { }
#[tokio::test]
async fn test_1000_concurrent_executions() { }
#[tokio::test]
async fn test_snapshot_restore_under_500ms() { }
```

## Missing Job Arg Structs
- RestoreSnapshotArgs
- StopInstanceArgs  
- PauseInstanceArgs
- ResumeInstanceArgs

## Test Infrastructure Needed
1. Mock executor for unit tests
2. Test containers with known behaviors
3. Chaos testing framework
4. Load testing harness
5. State verification utilities

## Coverage Goals
- Line coverage: > 80%
- Branch coverage: > 70%
- Integration test coverage: All 12 jobs
- E2E test coverage: 5 critical user journeys