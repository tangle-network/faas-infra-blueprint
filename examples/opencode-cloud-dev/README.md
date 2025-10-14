# OpenCode Cloud Development Environment

A containerized OpenCode server that accepts ANY prompt and returns AI-generated responses using the grok-code-fast-1 model (free tier).

## Prerequisites

- **Docker** - Required for building and running the OpenCode container
  - Download from: https://www.docker.com/products/docker-desktop
- **Docker Image** - The `opencode-chat:latest` image will be built automatically by build.rs
  - First build takes ~5-10 minutes (npm install, OpenCode CLI installation)
  - Subsequent builds are cached

## Architecture

**Container Stack:**
- **OpenCode Server** (port 5173, internal) - Runs via OpenCode SDK
- **Express Proxy** (port 4096, external) - Exposes chat API

**Technology:**
- Uses `@opencode-ai/sdk` for OpenCode server/client
- Node.js 20 slim container
- grok-code-fast-1 model (free tier, no API key required)
- Streaming SSE responses

## Features

✅ Accepts **ANY** prompt (no hardcoded responses)
✅ Real AI responses from OpenCode with Grok model
✅ Streaming responses via Server-Sent Events
✅ Containerized deployment
✅ Automatic Docker image building via build.rs
✅ Works with FaaS SDK

## Running

```bash
# Build (first time only - builds Docker image automatically)
cargo build --release --package opencode-cloud-dev

# Run the example
cargo run --release --package opencode-cloud-dev

# Or build and run container manually
cd opencode-server
docker build -t opencode-chat:latest .
docker run --rm -p 4096:4096 opencode-chat:latest

# Test it with curl
curl -X POST http://localhost:4096/api/chat \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Write hello world in Python"}'
```

## Build Process

The `build.rs` script automatically:
1. Checks if Docker is installed
2. Checks if `opencode-chat:latest` image exists
3. If not, builds it from `opencode-server/Dockerfile`
4. Provides helpful error messages if Docker is unavailable

## API Endpoints

- `GET /health` - Health check with ready status
- `POST /api/chat` - Send a prompt, receive streaming response
- `POST /api/prompt` - Alternative prompt endpoint

## Example Prompts

The server accepts absolutely ANY prompt:
- "Write a Python function to calculate fibonacci numbers"
- "Create a TypeScript React component for a dashboard"
- "What is 5+7?"
- "Explain quantum computing in simple terms"
- "Build a production-ready Rust GUI application"

## Container Structure

```
opencode-server/
├── Dockerfile          # Debian-based Node.js with OpenCode CLI
├── package.json        # Node dependencies (@opencode-ai/sdk)
├── start-server.sh     # Starts OpenCode CLI + Express servers
└── src/
    └── server.js       # Express proxy that uses OpenCode client
```

## How It Works

1. **Startup**: `start-server.sh` launches OpenCode CLI server on port 5173
2. **Express Server**: Starts on port 4096, creates OpenCode client
3. **Request Flow**:
   - Client sends prompt to `/api/chat`
   - Express forwards to OpenCode server
   - OpenCode processes with grok-code-fast-1
   - Response streamed back via SSE
4. **Response Format**: Extracts text from `response.parts[]` array
