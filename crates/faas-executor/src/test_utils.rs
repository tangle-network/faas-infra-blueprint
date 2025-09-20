/// Test utilities for conditional test execution
use std::process::Command;

pub fn has_docker() -> bool {
    // Check if docker command exists
    Command::new("docker")
        .arg("info")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn has_firecracker() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }

    Command::new("firecracker")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn has_criu() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }

    Command::new("criu")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn has_kvm() -> bool {
    cfg!(target_os = "linux") && std::path::Path::new("/dev/kvm").exists()
}

/// Macro to skip tests when requirements aren't met
#[macro_export]
macro_rules! require_docker {
    () => {
        if !$crate::test_utils::has_docker() {
            eprintln!("Test ignored: Docker not available");
            return;
        }
    };
}

#[macro_export]
macro_rules! require_firecracker {
    () => {
        if !$crate::test_utils::has_firecracker() {
            eprintln!("Test ignored: Firecracker not available (Linux with KVM required)");
            return;
        }
    };
}

#[macro_export]
macro_rules! require_criu {
    () => {
        if !$crate::test_utils::has_criu() {
            eprintln!("Test ignored: CRIU not available (Linux required)");
            return;
        }
    };
}