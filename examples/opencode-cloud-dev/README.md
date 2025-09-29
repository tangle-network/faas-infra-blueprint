# OpenCode Cloud Development Environment

Run OpenCode Server as a cloud-based AI agent development environment using our FaaS platform. This example demonstrates how to create VMs with OpenCode running, enabling cloud-based development for AI agent projects.

## Features

- 🚀 **Cloud-based OpenCode Server** - Deploy OpenCode server in Docker/VM containers
- 🤖 **AI Agent Development** - Pre-configured environment for building AI agents
- 📡 **HTTP API Access** - Full OpenCode server API for programmatic control
- 🔧 **Multiple Project Templates** - React, Next.js, Rust, Python ML setups
- 💾 **Persistent Storage** - Keep your work between sessions
- 🌐 **Remote Access** - Access your dev environment from anywhere

## Architecture

```
┌─────────────────┐     HTTP API      ┌──────────────────┐
│   Your Local    │◄──────────────────►│  OpenCode Server │
│   Environment   │                    │   (Cloud VM)     │
└─────────────────┘                    └──────────────────┘
                                               │
                                               ▼
                                        ┌──────────────────┐
                                        │  AI Agents       │
                                        │  Development     │
                                        │  Workspace       │
                                        └──────────────────┘
```

## Quick Start

```bash
# Run the example
cargo run --package opencode-cloud-dev

# The example will:
# 1. Deploy OpenCode server in a cloud container
# 2. Setup a development workspace
# 3. Configure AI agent environment
# 4. Connect to the server API
# 5. Create a development session
```

## Configuration

```rust
let config = OpenCodeConfig {
    port: 4096,                        // OpenCode server port
    hostname: "0.0.0.0".to_string(),    // Listen on all interfaces
    persistent_storage: true,           // Enable persistent storage
    memory_mb: 8192,                   // 8GB RAM
    cpu_cores: 4,                      // 4 CPU cores
    env_vars: HashMap::new(),          // Environment variables
};
```

## Available Project Templates

### React TypeScript
```rust
manager.create_workspace("react").await?;
```

### Next.js
```rust
manager.create_workspace("nextjs").await?;
```

### Rust
```rust
manager.create_workspace("rust").await?;
```

### Python ML
```rust
manager.create_workspace("python-ml").await?;
```

## OpenCode Server API

The deployed server exposes the full OpenCode HTTP API at `http://<instance-id>:4096`

### Key Endpoints

| Endpoint | Description |
|----------|-------------|
| `/app` | Get app information |
| `/session` | Manage development sessions |
| `/session/{id}/message` | Send AI chat messages |
| `/session/{id}/shell` | Execute shell commands |
| `/find/file` | Search for files |
| `/file` | Read/write files |
| `/agent` | List available AI agents |
| `/doc` | OpenAPI 3.1 specification |

## AI Agent Development

The environment comes pre-configured for AI agent development:

```typescript
// Example agent in /workspace/agents/src/sample-agent.ts
import { Agent, Context, Tool } from '@opencode/sdk';

export class SampleAgent extends Agent {
    name = 'SampleAgent';
    description = 'A sample AI agent for development';

    async execute(context: Context): Promise<void> {
        // Your agent logic here
    }
}
```

## Using the OpenCode Client

```rust
// Connect to your cloud OpenCode server
let client = OpenCodeServerClient::new(server_url);

// Get app info
let info = client.get_app_info().await?;

// Create a session
let session_id = client.create_session(Some("My Project")).await?;

// Send AI messages
let response = client.send_message(
    &session_id,
    "Help me build a REST API"
).await?;

// Execute commands
let result = client.execute_command(
    &session_id,
    "npm test"
).await?;

// Search files
let files = client.find_files("*.ts").await?;

// Read file content
let content = client.read_file("/workspace/src/app.ts").await?;
```

## Advanced Usage

### Custom Docker Image

```rust
let dockerfile = r#"
FROM node:20-alpine
RUN npm install -g @opencode/cli your-custom-tools
# Your custom setup
CMD ["opencode", "serve"]
"#;
```

### Persistent Development Sessions

```rust
// Save session state
let session_data = client.export_session(&session_id).await?;

// Restore session later
client.import_session(session_data).await?;
```

### Multi-Agent Collaboration

```rust
// Deploy multiple agents
let agents = vec![
    "code-reviewer",
    "test-generator",
    "documentation-writer",
];

for agent in agents {
    manager.deploy_agent(agent).await?;
}
```

## Security Considerations

- API keys are passed as environment variables
- Use HTTPS in production
- Implement authentication for multi-user scenarios
- Regular security updates for base images

## Performance

- **Cold Start**: ~30 seconds (Docker image pull + OpenCode setup)
- **Warm Start**: ~2 seconds (pre-warmed containers)
- **API Response**: <100ms for most operations
- **File Operations**: Near-instant for workspace files

## Troubleshooting

### Server Won't Start
- Check Docker is running
- Verify port 4096 is available
- Ensure sufficient memory allocation

### Can't Connect to API
- Wait for server initialization (30-60 seconds)
- Check network connectivity
- Verify firewall rules

### Session Errors
- Ensure AI provider credentials are set
- Check model availability
- Verify API rate limits

## Next Steps

1. **Custom Agents** - Build specialized AI agents for your workflow
2. **IDE Integration** - Connect VS Code or other IDEs to your cloud instance
3. **Team Collaboration** - Share sessions with team members
4. **CI/CD Integration** - Automate testing with cloud dev environments
5. **Production Deployment** - Deploy agents to production FaaS

## Resources

- [OpenCode Documentation](https://docs.opencode.dev)
- [OpenAPI Spec](http://localhost:4096/doc)
- [Agent Development Guide](https://docs.opencode.dev/agents)
- [FaaS Platform Docs](../README.md)