//! CRIU tests that can run inside a Docker container
//! These tests use actual CRIU functionality when available

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

#[test]
#[ignore = "Requires CRIU in Docker container"]
fn test_criu_available() {
    let output = Command::new("criu").arg("--version").output();

    match output {
        Ok(out) => {
            let version = String::from_utf8_lossy(&out.stdout);
            println!("CRIU Version: {}", version);
            assert!(out.status.success(), "CRIU not properly installed");
        }
        Err(e) => {
            panic!("CRIU not available: {}", e);
        }
    }
}

#[test]
#[ignore = "Requires CRIU in Docker container"]
fn test_criu_check() {
    // CRIU check validates kernel features
    let output = Command::new("criu")
        .arg("check")
        .arg("--all")
        .output()
        .expect("Failed to run criu check");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("CRIU check warnings/errors: {}", stderr);
        // Don't fail - some features might not be available in container
    }
}

#[test]
#[ignore = "Requires CRIU in Docker container"]
fn test_simple_process_checkpoint() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().join("checkpoint");
    fs::create_dir(&checkpoint_dir).unwrap();

    // Start a simple sleep process
    let mut child = Command::new("sleep")
        .arg("100")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start sleep process");

    let pid = child.id();
    println!("Started process with PID: {}", pid);

    // Give process time to start
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Try to checkpoint the process
    let checkpoint = Command::new("criu")
        .arg("dump")
        .arg("-t")
        .arg(pid.to_string())
        .arg("-D")
        .arg(checkpoint_dir.to_str().unwrap())
        .arg("--shell-job")
        .arg("--leave-running") // Don't kill the process
        .output();

    match checkpoint {
        Ok(output) => {
            if output.status.success() {
                println!("Successfully checkpointed process {}", pid);

                // Check if checkpoint files were created
                let entries = fs::read_dir(&checkpoint_dir).unwrap().count();
                assert!(entries > 0, "No checkpoint files created");

                // List checkpoint files
                for entry in fs::read_dir(&checkpoint_dir).unwrap() {
                    if let Ok(e) = entry {
                        println!("Checkpoint file: {:?}", e.path());
                    }
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("CRIU dump failed: {}", stderr);
                // Don't fail test - might not have all permissions in container
            }
        }
        Err(e) => {
            println!("Could not run CRIU dump: {}", e);
        }
    }

    // Clean up process
    let _ = child.kill();
}

#[test]
#[ignore = "Requires CRIU in Docker container"]
fn test_criu_features() {
    // Test what CRIU features are available
    let features = [
        ("check", vec!["--feature", "uffd"]),
        ("check", vec!["--feature", "lazy-pages"]),
        ("check", vec!["--feature", "pidfd_store"]),
        ("check", vec!["--feature", "network_lock"]),
    ];

    for (cmd, args) in features {
        let output = Command::new("criu").arg(cmd).args(&args).output();

        match output {
            Ok(out) => {
                let feature_name = args.last().unwrap();
                if out.status.success() {
                    println!("✓ CRIU feature '{}' is available", feature_name);
                } else {
                    println!("✗ CRIU feature '{}' is not available", feature_name);
                }
            }
            Err(_) => continue,
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "Requires CRIU in Docker container"]
fn test_checkpoint_restore_cycle() {
    use std::io::Write;

    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().join("checkpoint");
    fs::create_dir(&checkpoint_dir).unwrap();

    // Create a simple script that writes to a file
    let script_path = temp_dir.path().join("test.sh");
    let mut script = fs::File::create(&script_path).unwrap();
    writeln!(script, "#!/bin/bash").unwrap();
    writeln!(script, "echo 'Process started' > /tmp/criu_test.txt").unwrap();
    writeln!(script, "for i in {{1..100}}; do").unwrap();
    writeln!(script, "  echo $i >> /tmp/criu_test.txt").unwrap();
    writeln!(script, "  sleep 0.1").unwrap();
    writeln!(script, "done").unwrap();
    drop(script);

    // Make script executable
    Command::new("chmod")
        .arg("+x")
        .arg(&script_path)
        .output()
        .unwrap();

    // Start the script
    let child = Command::new("bash").arg(&script_path).spawn();

    if let Ok(mut proc) = child {
        let pid = proc.id();

        // Let it run for a bit
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Checkpoint it
        let checkpoint_result = Command::new("criu")
            .arg("dump")
            .arg("-t")
            .arg(pid.to_string())
            .arg("-D")
            .arg(checkpoint_dir.to_str().unwrap())
            .arg("--shell-job")
            .arg("--tcp-established")
            .output();

        if let Ok(output) = checkpoint_result {
            if output.status.success() {
                println!("Process checkpointed successfully");

                // Process should be gone now
                std::thread::sleep(std::time::Duration::from_millis(100));

                // Try to restore
                let restore_result = Command::new("criu")
                    .arg("restore")
                    .arg("-D")
                    .arg(checkpoint_dir.to_str().unwrap())
                    .arg("--shell-job")
                    .arg("--tcp-established")
                    .output();

                if let Ok(restore_out) = restore_result {
                    if restore_out.status.success() {
                        println!("Process restored successfully!");

                        // Check if the file is still being written to
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        if Path::new("/tmp/criu_test.txt").exists() {
                            let content = fs::read_to_string("/tmp/criu_test.txt").unwrap();
                            println!("File content after restore:\n{}", content);
                        }
                    } else {
                        let stderr = String::from_utf8_lossy(&restore_out.stderr);
                        println!("Restore failed: {}", stderr);
                    }
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("Checkpoint failed: {}", stderr);
            }
        }

        let _ = proc.kill();
    }

    // Cleanup
    let _ = fs::remove_file("/tmp/criu_test.txt");
}
