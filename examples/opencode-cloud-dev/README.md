# OpenCode Cloud Development Environment

A containerized OpenCode server that accepts ANY prompt and returns AI-generated responses using the grok-code-fast-1 model (free tier).

## Architecture

**Container Stack:**
- **OpenCode Server** (port 5173, internal) - Runs via OpenCode SDK
- **Express Proxy** (port 4096, external) - Exposes chat API

**Technology:**
- Uses `@opencode-ai/sdk` for OpenCode server/client
- Debian-based Node.js 20 container
- grok-code-fast-1 model (free tier, no API key required)
- Streaming SSE responses

## Features

✅ Accepts **ANY** prompt (no hardcoded responses)
✅ Real AI responses from OpenCode with Grok model
✅ Streaming responses via Server-Sent Events
✅ Containerized deployment
✅ Works with FaaS SDK

## Running

```bash
# Build and run
cargo run --release

# Or run container directly
docker build -t opencode-chat:latest opencode-server/
docker run --rm -p 4096:4096 opencode-chat:latest

# Test it
curl -X POST http://localhost:4096/api/chat \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Write hello world in Python"}'
```

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
