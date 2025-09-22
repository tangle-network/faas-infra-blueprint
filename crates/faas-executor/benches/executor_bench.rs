//! Production performance benchmarks for FaaS executor
//! Measures cold start, warm start, and throughput

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use faas_common::{SandboxConfig, SandboxExecutor};
use faas_executor::{config::ExecutorConfig, DockerExecutor};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

/// Benchmark cold start performance
fn bench_cold_start(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let executor = Arc::new(DockerExecutor::new());

    let config = SandboxConfig {
        function_id: "bench-cold".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "hello".to_string()],
        env_vars: None,
        payload: vec![],
    };

    c.bench_function("cold_start_docker", |b| {
        b.to_async(&rt).iter(|| async {
            let exec = executor.clone();
            let cfg = config.clone();
            black_box(exec.execute(cfg).await)
        });
    });
}

/// Benchmark warm start performance with pre-pulled images
fn bench_warm_start(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let executor = Arc::new(DockerExecutor::new());

    // Pre-warm by running once
    rt.block_on(async {
        let _ = executor
            .execute(SandboxConfig {
                function_id: "warmup".to_string(),
                source: "alpine:latest".to_string(),
                command: vec!["echo".to_string(), "warmup".to_string()],
                env_vars: None,
                payload: vec![],
            })
            .await;
    });

    let config = SandboxConfig {
        function_id: "bench-warm".to_string(),
        source: "alpine:latest".to_string(),
        command: vec!["echo".to_string(), "hello".to_string()],
        env_vars: None,
        payload: vec![],
    };

    c.bench_function("warm_start_docker", |b| {
        b.to_async(&rt).iter(|| async {
            let exec = executor.clone();
            let cfg = config.clone();
            black_box(exec.execute(cfg).await)
        });
    });
}

/// Benchmark concurrent execution throughput
fn bench_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let executor = Arc::new(DockerExecutor::new());

    let mut group = c.benchmark_group("throughput");

    for concurrency in [1, 10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            &concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter(|| async move {
                    let mut handles = vec![];

                    for i in 0..concurrency {
                        let exec = executor.clone();
                        let handle = tokio::spawn(async move {
                            exec.execute(SandboxConfig {
                                function_id: format!("bench-{}", i),
                                source: "alpine:latest".to_string(),
                                command: vec!["echo".to_string(), format!("{}", i)],
                                env_vars: None,
                                payload: vec![],
                            })
                            .await
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        black_box(handle.await);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark different payload sizes
fn bench_payload_sizes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let executor = Arc::new(DockerExecutor::new());

    let mut group = c.benchmark_group("payload_size");

    for size_kb in [1, 10, 100, 1000] {
        let payload = vec![0u8; size_kb * 1024];

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}KB", size_kb)),
            &payload,
            |b, payload| {
                b.to_async(&rt).iter(|| async {
                    let exec = executor.clone();
                    black_box(
                        exec.execute(SandboxConfig {
                            function_id: "bench-payload".to_string(),
                            source: "alpine:latest".to_string(),
                            command: vec!["cat".to_string()],
                            env_vars: None,
                            payload: payload.clone(),
                        })
                        .await,
                    )
                });
            },
        );
    }

    group.finish();
}

/// Benchmark snapshot/restore performance (Linux only)
#[cfg(target_os = "linux")]
fn bench_snapshot_restore(c: &mut Criterion) {
    use faas_executor::criu::{CriuConfig, CriuManager};

    let rt = Runtime::new().unwrap();

    // Only run if CRIU is available
    let criu = rt.block_on(async { CriuManager::new(CriuConfig::default()).await.ok() });

    if let Some(criu_manager) = criu {
        let mut group = c.benchmark_group("snapshot_restore");

        // Start a test process
        let pid = std::process::id();

        group.bench_function("criu_checkpoint", |b| {
            b.to_async(&rt).iter(|| async {
                let checkpoint_id = format!("bench-{}", uuid::Uuid::new_v4());
                let result = criu_manager.checkpoint(pid, &checkpoint_id).await;
                // Clean up
                let _ = criu_manager.delete_checkpoint(&checkpoint_id).await;
                black_box(result)
            });
        });

        group.finish();
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(30));
    targets = bench_cold_start, bench_warm_start, bench_throughput, bench_payload_sizes
);

#[cfg(target_os = "linux")]
criterion_group!(
    name = linux_benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(30));
    targets = bench_snapshot_restore
);

#[cfg(not(target_os = "linux"))]
criterion_main!(benches);

#[cfg(target_os = "linux")]
criterion_main!(benches, linux_benches);
