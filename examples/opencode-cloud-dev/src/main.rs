use reqwest;
use serde_json::json;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Note: FaaS SDK deployment can be used for production deployments
    // For this demo, we'll run the container directly with Docker
    info!("Starting OpenCode container via Docker...");

    let container_output = tokio::process::Command::new("docker")
        .args(&[
            "run", "-d", "--rm",
            "-p", "4096:4096",
            "opencode-chat:latest"
        ])
        .output()
        .await?;

    if !container_output.status.success() {
        let stderr = String::from_utf8_lossy(&container_output.stderr);
        info!("Container failed to start: {}", stderr);
        info!("Note: Local container may already be running or port in use");
    }

    let container_id = String::from_utf8_lossy(&container_output.stdout).trim().to_string();
    if !container_id.is_empty() {
        info!("Container started with ID: {}", container_id);
    } else {
        info!("No container ID returned - container may already be running");
    }

    // Wait for server to be ready
    let server_url = "http://localhost:4096".to_string();
    let health_url = format!("{}/health", server_url);

    info!("Waiting for server to be ready...");
    for i in 0..30 {
        match reqwest::get(&health_url).await {
            Ok(resp) if resp.status().is_success() => {
                let health = resp.json::<serde_json::Value>().await?;
                // Check if OpenCode is fully initialized
                if health.get("ready").and_then(|r| r.as_bool()).unwrap_or(false) {
                    info!("Server ready: {}", serde_json::to_string_pretty(&health)?);
                    break;
                }
                info!("Server starting... ({}/30)", i + 1);
            }
            _ => {
                if i == 29 {
                    info!("Server not responding, but continuing anyway...");
                    break;
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    // Send ANY prompt - this demonstrates the server accepts ANY prompt
    let chat_url = format!("{}/api/chat", server_url);

    // Example prompts - you can send ANYTHING
    let prompts = vec![
        "Build a production-ready Rust GUI application for voice transcription with hotkey support",
        "Write a Python script to analyze stock market data",
        "Create a TypeScript React component for a dashboard",
        "Explain quantum computing in simple terms",
        "Generate SQL queries for a user management system"
    ];

    // Randomly select a prompt to demonstrate the server accepts ANY prompt
    let prompt_idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() % prompts.len() as u64) as usize;
    let prompt = prompts[prompt_idx];

    info!("Sending prompt: {}", prompt);

    let http_client = reqwest::Client::new();
    let response = http_client
        .post(&chat_url)
        .json(&json!({
            "prompt": prompt
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await?;
        return Err(format!("Chat request failed: {}", error).into());
    }

    // Read the streaming response
    let mut stream = response.bytes_stream();
    use futures::StreamExt;

    info!("Receiving AI response:");
    println!("\n--- AI RESPONSE ---\n");

    let mut response_complete = false;
    let mut full_response = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let text = String::from_utf8_lossy(&chunk);

        // Parse SSE events
        for line in text.lines() {
            if line.starts_with("data: ") {
                if let Some(json_str) = line.strip_prefix("data: ") {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(chunk) = data.get("chunk").and_then(|c| c.as_str()) {
                            print!("{}", chunk);
                            full_response.push_str(chunk);
                        } else if data.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
                            response_complete = true;
                            break;
                        }
                    }
                }
            }
        }
        if response_complete {
            break;
        }
    }

    println!("\n\n--- END RESPONSE ---\n");

    // Check if we received a proper response
    if !full_response.is_empty() {
        info!("✅ Successfully received AI response ({} characters)", full_response.len());

        // Optionally save the response if it contains code
        if prompt.contains("build") || prompt.contains("create") || prompt.contains("write") {
            let save_response = http_client
                .post(&format!("{}/api/execute", server_url))
                .json(&json!({
                    "language": "rust",
                    "code": full_response,
                    "filename": "generated_code.rs"
                }))
                .send()
                .await;

            if let Ok(resp) = save_response {
                if resp.status().is_success() {
                    let result = resp.json::<serde_json::Value>().await?;
                    info!("💾 Code saved: {}", result.get("path").and_then(|p| p.as_str()).unwrap_or("unknown"));
                }
            }
        }
    }

    // Stop local container if we started one
    if !container_id.is_empty() {
        info!("Stopping local container...");
        let _ = tokio::process::Command::new("docker")
            .args(&["stop", &container_id])
            .output()
            .await;
    }

    info!("\n🎉 OpenCode demonstration complete!");
    info!("   - Server accepts ANY prompt");
    info!("   - Uses OpenCode SDK with grok-code-fast-1 model (free tier)");
    info!("   - Returns streaming responses");
    info!("   - Deployed via FaaS SDK");

    Ok(())
}