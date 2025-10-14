use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=opencode-server/Dockerfile");
    println!("cargo:rerun-if-changed=opencode-server/src");
    println!("cargo:rerun-if-changed=opencode-server/package.json");

    // Check if docker is available
    let docker_check = Command::new("docker")
        .arg("--version")
        .output();

    if docker_check.is_err() {
        println!("cargo:warning=Docker is not available. Please install Docker to run this example.");
        println!("cargo:warning=Download from: https://www.docker.com/products/docker-desktop");
        return;
    }

    // Check if opencode-chat:latest image exists
    let image_check = Command::new("docker")
        .args(["image", "inspect", "opencode-chat:latest"])
        .output();

    if image_check.is_ok() && image_check.unwrap().status.success() {
        println!("cargo:warning=Docker image 'opencode-chat:latest' found");
        return;
    }

    // Image doesn't exist, try to build it
    println!("cargo:warning=Building Docker image 'opencode-chat:latest'...");
    println!("cargo:warning=This may take a few minutes on first build (npm install, etc)");

    let build_result = Command::new("docker")
        .args([
            "build",
            "-t",
            "opencode-chat:latest",
            "-f",
            "opencode-server/Dockerfile",
            "opencode-server",
        ])
        .status();

    match build_result {
        Ok(status) if status.success() => {
            println!("cargo:warning=✅ Successfully built Docker image 'opencode-chat:latest'");
        }
        Ok(status) => {
            println!("cargo:warning=❌ Docker build failed with exit code: {:?}", status.code());
            println!("cargo:warning=To build manually, run:");
            println!("cargo:warning=  cd examples/opencode-cloud-dev/opencode-server && docker build -t opencode-chat:latest .");
        }
        Err(e) => {
            println!("cargo:warning=❌ Failed to run docker build: {}", e);
            println!("cargo:warning=To build manually, run:");
            println!("cargo:warning=  cd examples/opencode-cloud-dev/opencode-server && docker build -t opencode-chat:latest .");
        }
    }
}
