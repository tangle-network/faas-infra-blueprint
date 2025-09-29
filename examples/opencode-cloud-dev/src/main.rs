use anyhow::Result;
use faas_sdk::{FaasClient, ExecuteRequest, Runtime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

/// Configuration for OpenCode cloud development environment
#[derive(Debug, Serialize, Deserialize)]
struct OpenCodeConfig {
    /// Port for OpenCode server
    port: u16,
    /// Hostname for OpenCode server
    hostname: String,
    /// Enable persistent storage
    persistent_storage: bool,
    /// Memory allocation in MB
    memory_mb: u32,
    /// CPU cores
    cpu_cores: u8,
    /// Environment variables
    env_vars: HashMap<String, String>,
}

impl Default for OpenCodeConfig {
    fn default() -> Self {
        Self {
            port: 4096,
            hostname: "0.0.0.0".to_string(),
            persistent_storage: true,
            memory_mb: 4096,
            cpu_cores: 2,
            env_vars: HashMap::new(),
        }
    }
}

/// OpenCode Cloud Development Environment Manager
struct OpenCodeCloudDev {
    client: FaasClient,
    config: OpenCodeConfig,
}

impl OpenCodeCloudDev {
    /// Create a new OpenCode cloud dev environment manager
    pub fn new(faas_endpoint: String, config: OpenCodeConfig) -> Self {
        let client = FaasClient::new(faas_endpoint, Runtime::Docker);
        Self { client, config }
    }

    /// Deploy OpenCode server in a cloud VM
    pub async fn deploy_server(&self) -> Result<String> {
        info!("Deploying OpenCode server with config: {:?}", self.config);

        // Create a custom Docker image with OpenCode installed
        let dockerfile = format!(
            r#"
FROM node:20-alpine

# Install system dependencies
RUN apk add --no-cache \
    git \
    curl \
    bash \
    python3 \
    make \
    g++ \
    openssh-client

# Install OpenCode globally
RUN npm install -g @opencode/cli

# Create workspace directory
WORKDIR /workspace

# Expose OpenCode server port
EXPOSE {}

# Set environment variables
ENV OPENCODE_PORT={}
ENV OPENCODE_HOSTNAME={}

# Start OpenCode server
CMD ["opencode", "serve", "--port", "{}", "--hostname", "{}"]
"#,
            self.config.port,
            self.config.port,
            self.config.hostname,
            self.config.port,
            self.config.hostname
        );

        // Execute the deployment
        let request = ExecuteRequest {
            function_id: "opencode-server".to_string(),
            source: dockerfile,
            command: vec![],
            env_vars: Some(self.config.env_vars.clone()),
            payload: vec![],
            timeout_ms: Some(300000), // 5 minutes for initial setup
            memory_mb: Some(self.config.memory_mb),
            cpu_cores: Some(self.config.cpu_cores),
        };

        let response = self.client.execute(request).await?;
        let instance_id = String::from_utf8(response.response)?;

        info!("OpenCode server deployed with instance ID: {}", instance_id);
        Ok(instance_id)
    }

    /// Create a development workspace with project initialization
    pub async fn create_workspace(&self, project_type: &str) -> Result<String> {
        info!("Creating workspace for project type: {}", project_type);

        let init_script = match project_type {
            "react" => {
                r#"
                npx create-react-app my-app --template typescript
                cd my-app
                npm install
                echo "React project initialized"
                "#
            }
            "nextjs" => {
                r#"
                npx create-next-app@latest my-app --typescript --tailwind --app
                cd my-app
                npm install
                echo "Next.js project initialized"
                "#
            }
            "rust" => {
                r#"
                cargo new my-project --bin
                cd my-project
                cargo build
                echo "Rust project initialized"
                "#
            }
            "python-ml" => {
                r#"
                mkdir my-ml-project && cd my-ml-project
                python3 -m venv venv
                source venv/bin/activate
                pip install numpy pandas scikit-learn jupyter torch transformers
                echo "Python ML project initialized"
                "#
            }
            _ => {
                r#"
                mkdir my-project && cd my-project
                git init
                echo "# Project" > README.md
                echo "Basic project initialized"
                "#
            }
        };

        let response = self.client
            .run_bash(&format!(
                r#"
                cd /workspace
                {}
                opencode init
                echo "Workspace ready at /workspace"
                "#,
                init_script
            ))
            .await?;

        Ok(String::from_utf8(response.response)?)
    }

    /// Connect to OpenCode server API
    pub async fn connect_to_server(&self, instance_id: &str) -> Result<OpenCodeServerClient> {
        let server_url = format!("http://{}:{}", instance_id, self.config.port);

        // Wait for server to be ready
        for _ in 0..30 {
            if let Ok(response) = reqwest::get(&format!("{}/app", server_url)).await {
                if response.status().is_success() {
                    info!("Connected to OpenCode server at {}", server_url);
                    return Ok(OpenCodeServerClient::new(server_url));
                }
            }
            sleep(Duration::from_secs(2)).await;
        }

        Err(anyhow::anyhow!("Failed to connect to OpenCode server"))
    }

    /// Setup AI agent development environment
    pub async fn setup_agent_environment(&self) -> Result<String> {
        info!("Setting up AI agent development environment");

        let setup_script = r#"
        # Install AI agent development dependencies
        npm install -g typescript ts-node nodemon

        # Create agent project structure
        mkdir -p /workspace/agents/{src,tests,docs}

        # Initialize TypeScript configuration
        cat > /workspace/agents/tsconfig.json << 'EOF'
        {
          "compilerOptions": {
            "target": "ES2020",
            "module": "commonjs",
            "lib": ["ES2020"],
            "outDir": "./dist",
            "rootDir": "./src",
            "strict": true,
            "esModuleInterop": true,
            "skipLibCheck": true,
            "forceConsistentCasingInFileNames": true
          }
        }
        EOF

        # Create sample agent
        cat > /workspace/agents/src/sample-agent.ts << 'EOF'
        import { Agent, Context, Tool } from '@opencode/sdk';

        export class SampleAgent extends Agent {
            name = 'SampleAgent';
            description = 'A sample AI agent for development';

            async execute(context: Context): Promise<void> {
                console.log('Agent executing with context:', context);
                // Agent logic here
            }
        }
        EOF

        # Create AGENTS.md
        cat > /workspace/agents/AGENTS.md << 'EOF'
        # AI Agents Configuration

        ## Available Agents

        ### SampleAgent
        - **Purpose**: Development and testing
        - **Capabilities**: Basic task execution
        - **Tools**: File system, HTTP requests

        ## Development Guide

        1. Create new agents in `src/` directory
        2. Follow the Agent interface from @opencode/sdk
        3. Test agents using the OpenCode server API
        4. Deploy agents to production FaaS
        EOF

        echo "AI agent environment ready"
        "#;

        let response = self.client.run_bash(setup_script).await?;
        Ok(String::from_utf8(response.response)?)
    }
}

/// Client for interacting with OpenCode server API
struct OpenCodeServerClient {
    base_url: String,
    client: reqwest::Client,
}

impl OpenCodeServerClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    /// Get app information
    pub async fn get_app_info(&self) -> Result<serde_json::Value> {
        let response = self.client
            .get(&format!("{}/app", self.base_url))
            .send()
            .await?
            .json()
            .await?;
        Ok(response)
    }

    /// Create a new session
    pub async fn create_session(&self, title: Option<String>) -> Result<String> {
        let mut body = HashMap::new();
        if let Some(t) = title {
            body.insert("title", t);
        }

        let response: serde_json::Value = self.client
            .post(&format!("{}/session", self.base_url))
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        Ok(response["id"].as_str().unwrap_or("").to_string())
    }

    /// Send a chat message to a session
    pub async fn send_message(&self, session_id: &str, content: &str) -> Result<serde_json::Value> {
        let body = serde_json::json!({
            "content": content,
            "model": "gpt-4",
            "provider": "openai"
        });

        let response = self.client
            .post(&format!("{}/session/{}/message", self.base_url, session_id))
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }

    /// Execute a shell command in a session
    pub async fn execute_command(&self, session_id: &str, command: &str) -> Result<serde_json::Value> {
        let body = serde_json::json!({
            "command": command
        });

        let response = self.client
            .post(&format!("{}/session/{}/shell", self.base_url, session_id))
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }

    /// Search for files
    pub async fn find_files(&self, query: &str) -> Result<Vec<String>> {
        let response: Vec<String> = self.client
            .get(&format!("{}/find/file", self.base_url))
            .query(&[("query", query)])
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }

    /// Read a file
    pub async fn read_file(&self, path: &str) -> Result<String> {
        let response: serde_json::Value = self.client
            .get(&format!("{}/file", self.base_url))
            .query(&[("path", path)])
            .send()
            .await?
            .json()
            .await?;

        Ok(response["content"].as_str().unwrap_or("").to_string())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Configuration
    let config = OpenCodeConfig {
        port: 4096,
        hostname: "0.0.0.0".to_string(),
        persistent_storage: true,
        memory_mb: 8192,
        cpu_cores: 4,
        env_vars: {
            let mut env = HashMap::new();
            env.insert("OPENAI_API_KEY".to_string(), "your-key-here".to_string());
            env.insert("NODE_ENV".to_string(), "development".to_string());
            env
        },
    };

    // Create OpenCode cloud dev manager
    let manager = OpenCodeCloudDev::new(
        "http://localhost:8080".to_string(),
        config,
    );

    // Deploy OpenCode server
    info!("Deploying OpenCode server in cloud...");
    let instance_id = manager.deploy_server().await?;
    info!("Server deployed: {}", instance_id);

    // Setup development workspace
    info!("Setting up TypeScript React workspace...");
    let workspace_result = manager.create_workspace("react").await?;
    info!("Workspace ready: {}", workspace_result);

    // Setup AI agent environment
    info!("Configuring AI agent development environment...");
    let agent_env = manager.setup_agent_environment().await?;
    info!("Agent environment: {}", agent_env);

    // Connect to OpenCode server
    info!("Connecting to OpenCode server API...");
    let client = manager.connect_to_server(&instance_id).await?;

    // Get app info
    let app_info = client.get_app_info().await?;
    info!("OpenCode app info: {:?}", app_info);

    // Create a development session
    let session_id = client.create_session(Some("AI Agent Development".to_string())).await?;
    info!("Created session: {}", session_id);

    // Send initial message
    let message = client.send_message(
        &session_id,
        "Help me create an AI agent that can analyze code and suggest improvements"
    ).await?;
    info!("Message response: {:?}", message);

    // Execute a command
    let cmd_result = client.execute_command(&session_id, "ls -la /workspace/agents").await?;
    info!("Command result: {:?}", cmd_result);

    // Search for files
    let files = client.find_files("*.ts").await?;
    info!("Found TypeScript files: {:?}", files);

    info!("OpenCode cloud development environment is ready!");
    info!("Access the server at: http://{}:4096", instance_id);
    info!("Use the OpenCode CLI or SDK to interact with the server");

    Ok(())
}