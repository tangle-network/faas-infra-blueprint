use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use faas_executor::platform::{Executor as PlatformExecutor, Mode, Request};
use std::time::Duration;

fn benchmark_checkpoint_restore(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let executor = runtime.block_on(async { PlatformExecutor::new().await.ok() });

    if executor.is_none() {
        eprintln!("Skipping benchmarks - executor unavailable");
        return;
    }

    let executor = executor.unwrap();

    let mut group = c.benchmark_group("checkpoint");

    // Benchmark ephemeral (no checkpoint)
    group.bench_function("ephemeral", |b| {
        b.to_async(&runtime).iter(|| async {
            let req = Request {
                id: "bench-1".to_string(),
                code: "echo hello".to_string(),
                mode: Mode::Ephemeral,
                env: "alpine:latest".to_string(),
                timeout: Duration::from_secs(5),
                checkpoint: None,
                branch_from: None,
            };
            executor.run(req).await
        });
    });

    // Benchmark cached (warm pool)
    group.bench_function("cached", |b| {
        b.to_async(&runtime).iter(|| async {
            let req = Request {
                id: "bench-2".to_string(),
                code: "echo hello".to_string(),
                mode: Mode::Cached,
                env: "alpine:latest".to_string(),
                timeout: Duration::from_secs(5),
                checkpoint: None,
                branch_from: None,
            };
            executor.run(req).await
        });
    });

    // Benchmark checkpointed
    group.bench_function("checkpointed", |b| {
        b.to_async(&runtime).iter(|| async {
            let req = Request {
                id: "bench-3".to_string(),
                code: "echo hello".to_string(),
                mode: Mode::Checkpointed,
                env: "alpine:latest".to_string(),
                timeout: Duration::from_secs(5),
                checkpoint: None,
                branch_from: None,
            };
            executor.run(req).await
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_checkpoint_restore);
criterion_main!(benches);
