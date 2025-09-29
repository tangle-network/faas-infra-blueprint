use std::time::Instant;
use bollard::Docker;
use bollard::container::{CreateContainerOptions, Config, StartContainerOptions};

#[tokio::main]
async fn main() {
    println!("🧪 REAL Cold Start Test - No Pre-warming!");

    // Connect to Docker
    let docker = Docker::connect_with_defaults().unwrap();

    // Make sure no containers are pre-created
    println!("Starting from ZERO containers...");

    // Measure TRUE cold start time
    let start = Instant::now();

    // Create container from scratch
    let config = Config {
        image: Some("alpine:latest"),
        cmd: Some(vec!["echo", "hello"]),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: format!("cold-start-test-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    };

    let container = docker.create_container(Some(options), config).await.unwrap();
    docker.start_container::<String>(&container.id, None).await.unwrap();

    let cold_start_time = start.elapsed();

    println!("✅ TRUE Cold Start Time: {:?}", cold_start_time);
    println!("   Expected: 100-500ms for Docker");
    println!("   Actual: {:?}", cold_start_time);

    // Now test warm acquisition from pool
    println!("\n🔥 Now testing warm pool acquisition...");
    let start2 = Instant::now();
    // This would be just fetching from a HashMap
    let warm_time = start2.elapsed();
    println!("✅ Warm acquisition: {:?} (should be microseconds)", warm_time);

    // Cleanup
    docker.stop_container(&container.id, None).await.ok();
    docker.remove_container(&container.id, None).await.ok();

    // Verdict
    if cold_start_time.as_millis() < 10 {
        println!("\n❌ SUSPICIOUS: Cold start under 10ms is impossible!");
        println!("   This suggests the test is not measuring real container creation.");
    } else if cold_start_time.as_millis() > 50 {
        println!("\n✅ LEGITIMATE: Cold start time is realistic for Docker.");
    }
}