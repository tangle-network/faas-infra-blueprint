use faas_common::SandboxExecutor;
use faas_executor::{DockerExecutor, Executor, WarmContainer};
use faas_executor::executor::{ExecutionStrategy, ContainerStrategy, DependencyLayer, DependencyType};
use faas_executor::docktopus::DockerBuilder;
use faas_orchestrator::Orchestrator;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

// Helper to setup blockchain-optimized orchestrator
async fn setup_blockchain_orchestrator() -> color_eyre::Result<Arc<Orchestrator>> {
    let docker_builder = DockerBuilder::new().await?;
    let docker_client = docker_builder.client();

    let strategy = ExecutionStrategy::Container(ContainerStrategy {
        warm_pools: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        max_pool_size: 10, // More containers for parallel builds
        docker: docker_client,
        build_cache_volumes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        dependency_layers: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        gpu_pools: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
    });

    let executor: Arc<dyn SandboxExecutor + Send + Sync> = Arc::new(
        Executor::new(strategy).await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to create executor: {}", e))?
    );
    Ok(Arc::new(Orchestrator::new(executor)))
}

#[tokio::test]
async fn blockchain_compilation_benchmark() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("=== BLOCKCHAIN COMPILATION BENCHMARK ===");

    let orchestrator = setup_blockchain_orchestrator().await?;

    // Give time for specialized container pre-warming
    info!("Pre-warming blockchain development containers...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    let test_cases = vec![
        // Test simple Rust compilation with serde (common dependency)
        ("rust_serde", "rust:latest", vec![
            "sh".to_string(), "-c".to_string(),
            r#"
            cargo init --name test_serde &&
            cd test_serde &&
            echo '[dependencies]' >> Cargo.toml &&
            echo 'serde = { version = "1.0", features = ["derive"] }' >> Cargo.toml &&
            echo 'serde_json = "1.0"' >> Cargo.toml &&
            echo 'use serde::{Serialize, Deserialize};

            #[derive(Serialize, Deserialize, Debug)]
            struct Block {
                height: u64,
                hash: String,
            }

            fn main() {
                let block = Block { height: 1, hash: "0x123".to_string() };
                println!("Block: {:?}", block);
            }' > src/main.rs &&
            time cargo build --release 2>&1
            "#.to_string()
        ]),

        // Test with tokio async runtime (common in blockchain)
        ("tokio_runtime", "rust:latest", vec![
            "sh".to_string(), "-c".to_string(),
            r#"
            cargo init --name test_tokio &&
            cd test_tokio &&
            echo '[dependencies]' >> Cargo.toml &&
            echo 'tokio = { version = "1", features = ["full"] }' >> Cargo.toml &&
            echo '#[tokio::main]
            async fn main() {
                println!("Tokio runtime started");
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                println!("Done");
            }' > src/main.rs &&
            time cargo build --release 2>&1
            "#.to_string()
        ]),

        // Test with crypto libraries
        ("crypto_libs", "rust:latest", vec![
            "sh".to_string(), "-c".to_string(),
            r#"
            cargo init --name test_crypto &&
            cd test_crypto &&
            echo '[dependencies]' >> Cargo.toml &&
            echo 'sha2 = "0.10"' >> Cargo.toml &&
            echo 'hex = "0.4"' >> Cargo.toml &&
            echo 'use sha2::{Sha256, Digest};

            fn main() {
                let mut hasher = Sha256::new();
                hasher.update(b"blockchain data");
                let result = hasher.finalize();
                println!("Hash: {}", hex::encode(result));
            }' > src/main.rs &&
            time cargo build --release 2>&1
            "#.to_string()
        ]),
    ];

    for (test_name, image, command) in test_cases {
        info!("\n--- Testing: {} ---", test_name);

        // First run (may use cache)
        let start1 = Instant::now();
        let result1 = orchestrator.schedule_execution(
            format!("{}-1", test_name),
            image.to_string(),
            command.clone(),
            None,
            Vec::new(),
        ).await;
        let duration1 = start1.elapsed();

        // Second run (should be cached)
        let start2 = Instant::now();
        let result2 = orchestrator.schedule_execution(
            format!("{}-2", test_name),
            image.to_string(),
            command.clone(),
            None,
            Vec::new(),
        ).await;
        let duration2 = start2.elapsed();

        match (result1, result2) {
            (Ok(_), Ok(_)) => {
                info!("=== {} RESULTS ===", test_name.to_uppercase());
                info!("First run: {:?} ({:.0}ms)", duration1, duration1.as_millis());
                info!("Second run (cached): {:?} ({:.0}ms)", duration2, duration2.as_millis());

                let speedup = duration1.as_millis() as f64 / duration2.as_millis() as f64;
                info!("Cache speedup: {:.2}x", speedup);

                if duration2.as_secs() < 1 {
                    info!("🚀 SUB-SECOND COMPILATION ACHIEVED!");
                } else if duration2.as_secs() < 5 {
                    info!("⚡ FAST COMPILATION: <5s");
                }
            }
            (Err(e), _) | (_, Err(e)) => warn!("Test failed: {}", e),
        }
    }

    Ok(())
}

#[tokio::test]
async fn compute_intensive_benchmark() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("=== COMPUTE INTENSIVE BENCHMARK ===");

    let orchestrator = setup_blockchain_orchestrator().await?;

    let test_cases = vec![
        // Cryptographic computation - more realistic
        ("crypto_hash", "alpine:latest", vec![
            "sh".to_string(), "-c".to_string(),
            r#"
            echo 'Computing SHA256 hashes...' &&
            for i in $(seq 1 10000); do echo "blockchain_data_$i" | sha256sum > /dev/null; done &&
            echo 'Done: 10K hashes computed'
            "#.to_string()
        ]),

        // Python numerical computation
        ("python_compute", "python:3-alpine", vec![
            "python".to_string(), "-c".to_string(),
            r#"
import time
import math
start = time.time()
# Simulate compute-intensive work
result = 0
for i in range(100000):
    result += math.sqrt(i) * math.sin(i)
print(f'Computation completed in {time.time()-start:.2f}s')
print(f'Result: {result:.2f}')
            "#.to_string()
        ]),

        // Go concurrent computation
        ("go_concurrent", "golang:1.21-alpine", vec![
            "sh".to_string(), "-c".to_string(),
            r#"
            cat > main.go << 'EOF'
package main
import (
    "fmt"
    "sync"
    "time"
)

func main() {
    start := time.Now()
    var wg sync.WaitGroup
    results := make([]int, 10)

    for i := 0; i < 10; i++ {
        wg.Add(1)
        go func(id int) {
            defer wg.Done()
            sum := 0
            for j := 0; j < 100000; j++ {
                sum += j * id
            }
            results[id] = sum
        }(i)
    }

    wg.Wait()
    fmt.Printf("Concurrent computation completed in %v\n", time.Since(start))
}
EOF
            go run main.go
            "#.to_string()
        ]),
    ];

    for (test_name, image, command) in test_cases {
        info!("\n--- Testing: {} ---", test_name);

        let start = Instant::now();
        let result = orchestrator.schedule_execution(
            test_name.to_string(),
            image.to_string(),
            command,
            None,
            Vec::new(),
        ).await;
        let duration = start.elapsed();

        match result {
            Ok(res) => {
                info!("=== {} RESULTS ===", test_name.to_uppercase());
                info!("Execution time: {:?} ({:.0}ms)", duration, duration.as_millis());

                if let Some(output) = res.response {
                    let output_str = String::from_utf8_lossy(&output);
                    info!("Output: {}", output_str.lines().take(5).collect::<Vec<_>>().join("\n"));
                }

                if duration.as_millis() < 500 {
                    info!("🚀 ULTRA-FAST COMPUTE: <500ms");
                } else if duration.as_secs() < 2 {
                    info!("⚡ FAST COMPUTE: <2s");
                }
            }
            Err(e) => warn!("Test failed: {}", e),
        }
    }

    Ok(())
}

#[tokio::test]
async fn parallel_build_test() -> color_eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("=== PARALLEL BUILD SCALABILITY TEST ===");

    let orchestrator = setup_blockchain_orchestrator().await?;

    // Pre-warm containers
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let num_parallel = 5;
    let start = Instant::now();

    // Launch parallel Rust compilations
    let mut handles = Vec::new();
    for i in 0..num_parallel {
        let orch = orchestrator.clone();
        let handle = tokio::spawn(async move {
            let task_start = Instant::now();
            let result = orch.schedule_execution(
                format!("parallel-build-{}", i),
                "rust:latest".to_string(),
                vec![
                    "sh".to_string(), "-c".to_string(),
                    format!(r#"
                    echo 'fn main() {{ println!("Build {}"); }}' > main.rs &&
                    cargo init --name build{} &&
                    cargo build --release
                    "#, i, i)
                ],
                None,
                Vec::new(),
            ).await;
            let task_duration = task_start.elapsed();
            (i, result, task_duration)
        });
        handles.push(handle);
    }

    // Wait for all builds
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await?);
    }

    let total_duration = start.elapsed();

    info!("=== PARALLEL BUILD RESULTS ===");
    info!("Total time for {} parallel builds: {:?}", num_parallel, total_duration);

    let successful = results.iter().filter(|(_, result, _)| result.is_ok()).count();
    info!("Successful builds: {}/{}", successful, num_parallel);

    for (i, result, duration) in &results {
        match result {
            Ok(_) => info!("Build {}: {:?}", i, duration),
            Err(e) => warn!("Build {} failed: {}", i, e),
        }
    }

    if let Some(max_duration) = results.iter().map(|(_, _, d)| d).max() {
        let parallelism_efficiency = (max_duration.as_millis() * num_parallel as u128) as f64
                                    / total_duration.as_millis() as f64;
        info!("Parallelism efficiency: {:.2}x", parallelism_efficiency);

        if parallelism_efficiency > 3.0 {
            info!("🚀 EXCELLENT PARALLEL PERFORMANCE!");
        } else if parallelism_efficiency > 2.0 {
            info!("✅ GOOD PARALLEL PERFORMANCE");
        }
    }

    Ok(())
}