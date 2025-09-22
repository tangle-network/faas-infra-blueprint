//! Platform setup tests - replaces test-faas bash scripts
//! Run with: cargo test --test platform_setup_test -- --ignored --nocapture

use std::process::Command;
use std::path::Path;

#[test]
#[ignore = "Downloads large binaries"]
fn test_download_firecracker_binaries() {
    if !cfg!(target_os = "linux") {
        eprintln!("Skipping Firecracker download - Linux only");
        return;
    }

    let arch = std::env::consts::ARCH;
    let fc_version = "v1.5.0";

    // Download Firecracker binary
    let fc_url = format!(
        "https://github.com/firecracker-microvm/firecracker/releases/download/{}/firecracker-{}-{}.tgz",
        fc_version, fc_version, arch
    );

    println!("Downloading Firecracker from: {}", fc_url);

    let output = Command::new("wget")
        .args(["-q", "-O", "/tmp/firecracker.tgz", &fc_url])
        .output()
        .expect("Failed to download Firecracker");

    assert!(output.status.success(), "Failed to download Firecracker");

    // Extract
    let output = Command::new("tar")
        .args(["-xzf", "/tmp/firecracker.tgz", "-C", "/tmp"])
        .output()
        .expect("Failed to extract Firecracker");

    assert!(output.status.success());
    assert!(Path::new(&format!("/tmp/release-{}-unknown-linux-musl/firecracker", fc_version)).exists());
}

#[test]
#[ignore = "Builds CRIU from source"]
fn test_build_criu() {
    if !cfg!(target_os = "linux") {
        eprintln!("Skipping CRIU build - Linux only");
        return;
    }

    // Check if already installed
    if Command::new("criu").arg("--version").output().is_ok() {
        println!("CRIU already installed");
        return;
    }

    println!("Building CRIU from source...");

    // Clone CRIU
    let output = Command::new("git")
        .args(["clone", "--depth=1", "--branch=v3.19",
               "https://github.com/checkpoint-restore/criu.git", "/tmp/criu"])
        .output()
        .expect("Failed to clone CRIU");

    if !output.status.success() {
        eprintln!("Failed to clone CRIU: {}", String::from_utf8_lossy(&output.stderr));
        return;
    }

    // Build CRIU
    let output = Command::new("make")
        .current_dir("/tmp/criu")
        .arg("-j4")
        .output()
        .expect("Failed to build CRIU");

    assert!(output.status.success(), "Failed to build CRIU");

    println!("CRIU built successfully");
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_docker_setup() {
    use bollard::Docker;

    let docker = Docker::connect_with_defaults()
        .expect("Docker not available");

    // Pull required images
    for image in ["alpine:latest", "ubuntu:latest"] {
        println!("Pulling image: {}", image);

        use bollard::image::CreateImageOptions;
        use futures::StreamExt;

        let options = CreateImageOptions {
            from_image: image,
            ..Default::default()
        };

        let mut stream = docker.create_image(Some(options), None, None);
        while let Some(result) = stream.next().await {
            if let Err(e) = result {
                eprintln!("Error pulling {}: {}", image, e);
            }
        }
    }

    println!("Docker setup complete");
}

#[test]
fn test_platform_capabilities() {
    println!("\n=== Platform Capabilities ===");
    println!("OS: {}", std::env::consts::OS);
    println!("Arch: {}", std::env::consts::ARCH);

    // Check KVM
    let has_kvm = Path::new("/dev/kvm").exists();
    println!("KVM: {}", if has_kvm { "Available" } else { "Not available (required for Firecracker)" });

    // Check Docker
    let has_docker = Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    println!("Docker: {}", if has_docker { "Available" } else { "Not available" });

    // Check CRIU
    let has_criu = Command::new("criu")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    println!("CRIU: {}", if has_criu { "Available" } else { "Not available" });

    // Check Firecracker
    let has_fc = Command::new("firecracker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    println!("Firecracker: {}", if has_fc { "Available" } else { "Not available" });

    println!("\n=== Recommended Setup ===");
    if cfg!(target_os = "macos") {
        println!("macOS detected - Only Docker executor will work");
        println!("For full capabilities, use a Linux machine with KVM");
    } else if cfg!(target_os = "linux") {
        if !has_kvm {
            println!("⚠️  KVM not available - Firecracker won't work");
            println!("   Enable virtualization in BIOS or use a bare metal Linux machine");
        }
        if !has_criu {
            println!("⚠️  CRIU not installed - Checkpoint/restore won't work");
            println!("   Run: cargo test --test platform_setup_test test_build_criu -- --ignored");
        }
    }
}