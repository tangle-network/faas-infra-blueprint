/// Automated tests for all examples
/// These tests verify that examples actually work

use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_gpu_service_example() {
    // Build the example first
    let output = Command::new("cargo")
        .args(&["build", "--package", "gpu-service-example", "--release"])
        .output()
        .expect("Failed to build gpu-service example");

    assert!(output.status.success(), "Failed to build gpu-service");

    // Start gateway
    let mut gateway = start_gateway();
    sleep(Duration::from_secs(3)).await;

    // Run the GPU service example
    let output = Command::new("cargo")
        .args(&["run", "--package", "gpu-service-example", "--release"])
        .output()
        .expect("Failed to run gpu-service");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify expected output
    assert!(stdout.contains("Loading") || stdout.contains("model"),
            "GPU service should mention loading models");

    gateway.kill().unwrap();
}

#[tokio::test]
async fn test_agent_branching_example() {
    // Build the example
    let output = Command::new("cargo")
        .args(&["build", "--package", "agent-branching-example", "--release"])
        .output()
        .expect("Failed to build agent-branching example");

    assert!(output.status.success(), "Failed to build agent-branching");

    // Start gateway
    let mut gateway = start_gateway();
    sleep(Duration::from_secs(3)).await;

    // Run the agent branching example
    let output = Command::new("cargo")
        .args(&["run", "--package", "agent-branching-example", "--release"])
        .output()
        .expect("Failed to run agent-branching");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify branching behavior
    assert!(stdout.contains("Branch") || stdout.contains("branch"),
            "Agent branching should mention branches");

    gateway.kill().unwrap();
}

#[tokio::test]
async fn test_quickstart_example() {
    // Build the example
    let output = Command::new("cargo")
        .args(&["build", "--package", "quickstart", "--release"])
        .output()
        .expect("Failed to build quickstart");

    assert!(output.status.success(), "Failed to build quickstart");

    // The quickstart doesn't need gateway, it uses executor directly
    // Run it and check output
    let output = Command::new("cargo")
        .args(&["run", "--package", "quickstart", "--release"])
        .output()
        .expect("Failed to run quickstart");

    assert!(output.status.success(), "Quickstart should run successfully");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "Quickstart should produce output");
}

#[tokio::test]
async fn test_python_scripts_exist() {
    // Check that Python scripts for GPU service exist
    let scripts = vec![
        "examples/gpu-service/scripts/load_model.py",
        "examples/gpu-service/scripts/inference.py",
    ];

    for script in scripts {
        assert!(
            std::path::Path::new(script).exists(),
            "Script {} should exist",
            script
        );
    }

    // Verify scripts are valid Python
    for script in scripts {
        let output = Command::new("python3")
            .args(&["-m", "py_compile", script])
            .output()
            .expect("Failed to check Python syntax");

        assert!(
            output.status.success(),
            "Python script {} has syntax errors",
            script
        );
    }
}

#[tokio::test]
async fn test_demo_repository_structure() {
    // Verify demo repository structure exists
    let demo_dirs = vec![
        "/Users/drew/webb/faas-demos",
        "/Users/drew/webb/faas-demos/python",
        "/Users/drew/webb/faas-demos/typescript",
    ];

    for dir in demo_dirs {
        assert!(
            std::path::Path::new(dir).is_dir(),
            "Demo directory {} should exist",
            dir
        );
    }

    // Check for demo files
    let demo_files = vec![
        "/Users/drew/webb/faas-demos/python/gpu_inference.py",
        "/Users/drew/webb/faas-demos/typescript/gpu-inference.ts",
        "/Users/drew/webb/faas-demos/SDK_REVIEW.md",
    ];

    for file in demo_files {
        assert!(
            std::path::Path::new(file).exists(),
            "Demo file {} should exist",
            file
        );
    }
}

fn start_gateway() -> std::process::Child {
    Command::new("cargo")
        .args(&["run", "--package", "faas-gateway-server", "--release"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("Failed to start gateway")
}