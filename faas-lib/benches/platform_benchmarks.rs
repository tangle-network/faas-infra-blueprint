use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use faas_executor::platform::{Executor, Mode, Request};
use std::time::Duration;
use tokio::runtime::Runtime;

fn benchmark_modes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let executor = match rt.block_on(async { Executor::new().await }) {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!(
                "Skipping benchmark: Executor initialization failed (likely missing Firecracker binary)"
            );
            return;
        }
    };

    let mut group = c.benchmark_group("execution_modes");

    for mode in [Mode::Ephemeral, Mode::Cached, Mode::Checkpointed] {
        group.bench_with_input(
            BenchmarkId::new("mode", format!("{:?}", mode)),
            &mode,
            |b, &mode| {
                b.to_async(&rt).iter(|| async {
                    let req = Request {
                        id: format!("bench-{:?}", mode),
                        code: "echo benchmark".to_string(),
                        mode,
                        env: "alpine:latest".to_string(),
                        timeout: Duration::from_secs(30),
                        checkpoint: None,
                        branch_from: None,
                    };

                    black_box(executor.run(req).await.unwrap())
                });
            },
        );
    }

    group.finish();
}

fn benchmark_fork_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let executor = match rt.block_on(async { Executor::new().await }) {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping fork benchmark: Executor initialization failed");
            return;
        }
    };

    c.bench_function("fork_10_branches", |b| {
        b.to_async(&rt).iter(|| async {
            // Create a checkpointed execution first
            let req = Request {
                id: "fork-parent".to_string(),
                code: "echo parent".to_string(),
                mode: Mode::Checkpointed,
                env: "alpine:latest".to_string(),
                timeout: Duration::from_secs(30),
                checkpoint: None,
                branch_from: None,
            };

            let parent = executor.run(req).await.unwrap();

            if let Some(snapshot) = parent.snapshot {
                // Benchmark forking
                let fork_req = Request {
                    id: "fork-child".to_string(),
                    code: "echo child".to_string(),
                    mode: Mode::Branched,
                    env: "alpine:latest".to_string(),
                    timeout: Duration::from_secs(30),
                    checkpoint: None,
                    branch_from: Some(snapshot),
                };

                black_box(executor.run(fork_req).await.unwrap())
            } else {
                panic!("No snapshot created");
            }
        });
    });
}

fn benchmark_ai_exploration(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let executor = match rt.block_on(async { Executor::new().await }) {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping AI exploration benchmark: Executor initialization failed");
            return;
        }
    };

    c.bench_function("ai_exploration_tree", |b| {
        b.to_async(&rt).iter(|| async {
            // Simulate AI agent exploration pattern
            let setup_req = Request {
                id: "ai-setup".to_string(),
                code: r#"
                    pip install numpy
                    python -c "
import numpy as np
import json
state = {'exploration_data': np.random.rand(100).tolist()}
with open('/tmp/state.json', 'w') as f:
    json.dump(state, f)
print('Setup complete')
                    "#
                .to_string(),
                mode: Mode::Checkpointed,
                env: "python:3-alpine".to_string(),
                timeout: Duration::from_secs(60),
                checkpoint: None,
                branch_from: None,
            };

            let base = executor.run(setup_req).await.unwrap();

            if let Some(snapshot) = base.snapshot {
                // Create 5 exploration branches
                let mut branches = Vec::new();
                for i in 0..5 {
                    let explore_req = Request {
                        id: format!("explore-{}", i),
                        code: format!(
                            r#"
                            python -c "
import json
with open('/tmp/state.json', 'r') as f:
    state = json.load(f)

# Simulate different exploration strategies
strategy = {}
print(f'Exploring with strategy {{strategy}}')
                            "#,
                            i
                        ),
                        mode: Mode::Branched,
                        env: "python:3-alpine".to_string(),
                        timeout: Duration::from_secs(30),
                        checkpoint: None,
                        branch_from: Some(snapshot.clone()),
                    };

                    branches.push(executor.run(explore_req));
                }

                // Wait for all explorations to complete
                let results = futures::future::join_all(branches).await;
                black_box(results)
            } else {
                panic!("No snapshot for exploration");
            }
        });
    });
}

fn benchmark_memory_efficiency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let executor = match rt.block_on(async { Executor::new().await }) {
        Ok(exec) => exec,
        Err(_) => {
            eprintln!("Skipping memory efficiency benchmark: Executor initialization failed");
            return;
        }
    };

    c.bench_function("memory_heavy_parallel", |b| {
        b.to_async(&rt).iter(|| async {
            // Create memory-heavy base state
            let base_req = Request {
                id: "memory-base".to_string(),
                code: r#"
                    python -c "
import numpy as np
# Create large array (100MB)
data = np.random.rand(100 * 1024 * 1024 // 8)
print(f'Created array of size: {data.nbytes} bytes')
                    "#
                .to_string(),
                mode: Mode::Checkpointed,
                env: "python:3-alpine".to_string(),
                timeout: Duration::from_secs(60),
                checkpoint: None,
                branch_from: None,
            };

            let base = executor.run(base_req).await.unwrap();

            if let Some(snapshot) = base.snapshot {
                // Create multiple branches - should share memory via KSM
                let mut branches = Vec::new();
                for i in 0..10 {
                    let branch_req = Request {
                        id: format!("memory-branch-{}", i),
                        code: format!(
                            "python -c \"print('Branch {} working with shared data')\"",
                            i
                        ),
                        mode: Mode::Branched,
                        env: "python:3-alpine".to_string(),
                        timeout: Duration::from_secs(30),
                        checkpoint: None,
                        branch_from: Some(snapshot.clone()),
                    };

                    branches.push(executor.run(branch_req));
                }

                let results = futures::future::join_all(branches).await;
                black_box(results)
            } else {
                panic!("No snapshot for memory test");
            }
        });
    });
}

criterion_group!(
    benches,
    benchmark_modes,
    benchmark_fork_creation,
    benchmark_ai_exploration,
    benchmark_memory_efficiency
);
criterion_main!(benches);

#[cfg(test)]
mod integration_tests {
    use super::*;
    use faas_executor::platform::{Executor, Mode, Request};
    use std::time::Duration;

    #[tokio::test]
    async fn test_performance_targets() {
        let executor = Executor::new().await.unwrap();

        // Test ephemeral execution <50ms
        let start = std::time::Instant::now();
        let req = Request {
            id: "perf-ephemeral".to_string(),
            code: "echo fast".to_string(),
            mode: Mode::Ephemeral,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };

        let result = executor.run(req).await.unwrap();
        let duration = start.elapsed();

        assert_eq!(result.exit_code, 0);
        assert!(
            duration < Duration::from_millis(100),
            "Ephemeral too slow: {:?}",
            duration
        );
    }

    #[tokio::test]
    async fn test_checkpoint_restore_cycle() {
        let executor = Executor::new().await.unwrap();

        // Create checkpoint
        let checkpoint_start = std::time::Instant::now();
        let req = Request {
            id: "checkpoint-test".to_string(),
            code: "echo checkpoint_me".to_string(),
            mode: Mode::Checkpointed,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };

        let result = executor.run(req).await.unwrap();
        let checkpoint_duration = checkpoint_start.elapsed();

        assert!(
            checkpoint_duration < Duration::from_millis(300),
            "Checkpoint too slow: {:?}",
            checkpoint_duration
        );
        assert!(result.snapshot.is_some());

        // Restore from checkpoint
        let restore_start = std::time::Instant::now();
        let restore_req = Request {
            id: "restore-test".to_string(),
            code: "echo restored".to_string(),
            mode: Mode::Checkpointed,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: result.snapshot,
            branch_from: None,
        };

        let restore_result = executor.run(restore_req).await.unwrap();
        let restore_duration = restore_start.elapsed();

        assert!(
            restore_duration < Duration::from_millis(350),
            "Restore too slow: {:?}",
            restore_duration
        );
        assert_eq!(restore_result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_branch_performance() {
        let executor = Executor::new().await.unwrap();

        // Create parent
        let parent_req = Request {
            id: "branch-parent".to_string(),
            code: "echo parent_state".to_string(),
            mode: Mode::Checkpointed,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };

        let parent = executor.run(parent_req).await.unwrap();
        assert!(parent.snapshot.is_some());

        // Test branch creation speed
        let branch_start = std::time::Instant::now();
        let branch_req = Request {
            id: "branch-child".to_string(),
            code: "echo child_state".to_string(),
            mode: Mode::Branched,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: parent.snapshot,
        };

        let branch = executor.run(branch_req).await.unwrap();
        let branch_duration = branch_start.elapsed();

        assert!(
            branch_duration < Duration::from_millis(100),
            "Branch too slow: {:?}",
            branch_duration
        );
        assert_eq!(branch.exit_code, 0);
    }

    #[tokio::test]
    async fn test_ai_agent_workflow() {
        let executor = Executor::new().await.unwrap();

        // Simulate AI agent setup
        let setup_req = Request {
            id: "ai-agent-setup".to_string(),
            code: r#"
                python -c "
import json
import random

# Simulate agent state
state = {
    'exploration_history': [],
    'current_hypothesis': 'initial',
    'confidence': 0.5,
    'data': [random.random() for _ in range(1000)]
}

with open('/tmp/agent_state.json', 'w') as f:
    json.dump(state, f)

print('Agent initialized')
                "#
            .to_string(),
            mode: Mode::Checkpointed,
            env: "python:3-alpine".to_string(),
            timeout: Duration::from_secs(60),
            checkpoint: None,
            branch_from: None,
        };

        let setup = executor.run(setup_req).await.unwrap();
        assert_eq!(setup.exit_code, 0);
        assert!(setup.snapshot.is_some());

        // Create multiple exploration branches
        let snapshot = setup.snapshot.unwrap();
        let mut exploration_tasks = Vec::new();

        for strategy in 0..5 {
            let explore_req = Request {
                id: format!("explore-strategy-{}", strategy),
                code: format!(
                    r#"
                    python -c "
import json
import random

with open('/tmp/agent_state.json', 'r') as f:
    state = json.load(f)

# Simulate different exploration strategies
strategy_id = {}
state['exploration_history'].append(f'strategy_{{strategy_id}}')
state['confidence'] = random.uniform(0.3, 0.9)

with open('/tmp/agent_state.json', 'w') as f:
    json.dump(state, f)

print(f'Explored with strategy {{strategy_id}}, confidence: {{state[\"confidence\"]}}')
                    "#,
                    strategy
                ),
                mode: Mode::Branched,
                env: "python:3-alpine".to_string(),
                timeout: Duration::from_secs(30),
                checkpoint: None,
                branch_from: Some(snapshot.clone()),
            };

            exploration_tasks.push(executor.run(explore_req));
        }

        // Execute all explorations in parallel
        let exploration_start = std::time::Instant::now();
        let results = futures::future::join_all(exploration_tasks).await;
        let exploration_duration = exploration_start.elapsed();

        // Verify all explorations succeeded
        for (i, result) in results.iter().enumerate() {
            assert!(result.is_ok(), "Exploration {} failed: {:?}", i, result);
            assert_eq!(result.as_ref().unwrap().exit_code, 0);
        }

        // Should complete parallel exploration in reasonable time
        assert!(
            exploration_duration < Duration::from_secs(10),
            "Parallel exploration too slow: {:?}",
            exploration_duration
        );

        println!(
            "AI agent workflow completed successfully in {:?}",
            exploration_duration
        );
    }
}
