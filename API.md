# FaaS Platform API Documentation

## Authentication

All API requests require authentication via API key. Include your API key in one of these ways:

### Bearer Token
```
Authorization: Bearer your_api_key_here
```

### X-API-Key Header
```
X-API-Key: your_api_key_here
```

## Base URL

```
https://api.faas.platform/api/v1
```

## Rate Limits

- **Requests per minute**: 100
- **Requests per hour**: 1000
- **Concurrent executions**: 10

Rate limit headers are included in responses:
- `X-RateLimit-Limit`: Maximum requests allowed
- `X-RateLimit-Remaining`: Requests remaining
- `X-RateLimit-Reset`: Time when limit resets

## Execution API

### Execute Code

Execute code in an isolated environment with specified mode.

```http
POST /execute
```

#### Request Body

```json
{
  "code": "console.log('Hello World')",
  "language": "javascript",
  "mode": "ephemeral",
  "resources": {
    "cpu_cores": 1,
    "memory_mb": 512,
    "timeout_ms": 30000
  },
  "env_vars": {
    "NODE_ENV": "production"
  },
  "files": {
    "input.txt": "base64_encoded_content"
  }
}
```

#### Parameters

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| code | string | Yes | Code to execute |
| language | string | Yes | Programming language (javascript, python, rust, go) |
| mode | string | No | Execution mode (ephemeral, cached, checkpointed, branched, persistent) |
| resources | object | No | Resource allocation |
| env_vars | object | No | Environment variables |
| files | object | No | Input files (base64 encoded) |

#### Response

```json
{
  "execution_id": "exec_abc123",
  "status": "completed",
  "exit_code": 0,
  "stdout": "Hello World\n",
  "stderr": "",
  "duration_ms": 145,
  "mode": "ephemeral",
  "cached": false,
  "cost": {
    "compute": 0.00012,
    "total": 0.00012
  }
}
```

### Stream Execution Output

Stream real-time output via WebSocket.

```http
GET /execute/stream
```

#### WebSocket Protocol

Connect to WebSocket and send commands:

```javascript
// Connect
ws = new WebSocket('wss://api.faas.platform/api/v1/stream/exec_123');

// Subscribe to execution
ws.send(JSON.stringify({
  type: 'subscribe',
  execution_id: 'exec_123'
}));

// Receive output
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  switch(msg.type) {
    case 'output':
      console.log(msg.data);
      break;
    case 'error':
      console.error(msg.error);
      break;
    case 'completed':
      console.log('Exit code:', msg.exit_code);
      break;
  }
};
```

## Snapshot API

### Create Snapshot

Create a checkpoint of execution state.

```http
POST /snapshots
```

#### Request Body

```json
{
  "execution_id": "exec_abc123",
  "metadata": {
    "name": "Initial state",
    "tags": ["v1", "stable"]
  }
}
```

#### Response

```json
{
  "snapshot_id": "snap_xyz789",
  "execution_id": "exec_abc123",
  "mode": "checkpointed",
  "created_at": "2024-01-15T10:30:00Z",
  "size_bytes": 1048576,
  "checksum": "sha256:abcdef..."
}
```

### List Snapshots

Get all snapshots for your account.

```http
GET /snapshots
```

#### Query Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| mode | string | Filter by mode |
| parent_id | string | Filter by parent snapshot |
| limit | integer | Number of results (default: 20) |
| offset | integer | Pagination offset |

#### Response

```json
{
  "snapshots": [
    {
      "snapshot_id": "snap_xyz789",
      "mode": "checkpointed",
      "created_at": "2024-01-15T10:30:00Z",
      "size_bytes": 1048576
    }
  ],
  "total": 42,
  "limit": 20,
  "offset": 0
}
```

### Restore Snapshot

Restore execution from a snapshot.

```http
POST /snapshots/{snapshot_id}/restore
```

#### Response

```json
{
  "execution_id": "exec_new456",
  "snapshot_id": "snap_xyz789",
  "status": "restored",
  "duration_ms": 180
}
```

### Delete Snapshot

Remove a snapshot permanently.

```http
DELETE /snapshots/{snapshot_id}
```

## Branch API

### Create Branch

Create a new branch from a snapshot.

```http
POST /branches
```

#### Request Body

```json
{
  "snapshot_id": "snap_xyz789",
  "metadata": {
    "name": "Feature branch",
    "description": "Testing new approach"
  }
}
```

#### Response

```json
{
  "branch_id": "branch_abc123",
  "snapshot_id": "snap_xyz789",
  "parent_branch": null,
  "created_at": "2024-01-15T10:35:00Z"
}
```

### List Branches

Get all branches.

```http
GET /branches
```

#### Query Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| snapshot_id | string | Filter by snapshot |
| parent_branch | string | Filter by parent |
| limit | integer | Number of results |
| offset | integer | Pagination offset |

### Merge Branches

Merge multiple branches into a new snapshot.

```http
POST /branches/merge
```

#### Request Body

```json
{
  "branch_ids": ["branch_123", "branch_456"],
  "strategy": "three_way",
  "metadata": {
    "name": "Merged result"
  }
}
```

## Instance API

### Start Instance

Launch a persistent instance.

```http
POST /instances
```

#### Request Body

```json
{
  "snapshot_id": "snap_xyz789",
  "resources": {
    "cpu_cores": 2,
    "memory_mb": 4096,
    "disk_gb": 20,
    "gpu_count": 0
  },
  "ttl": 3600,
  "auto_stop": true,
  "metadata": {
    "name": "Development server",
    "project": "my-app"
  }
}
```

#### Response

```json
{
  "instance_id": "inst_def456",
  "state": "starting",
  "endpoints": {
    "ssh": "ssh://instance123.faas.io:22001"
  },
  "created_at": "2024-01-15T10:40:00Z"
}
```

### Get Instance

Get instance details.

```http
GET /instances/{instance_id}
```

#### Response

```json
{
  "instance_id": "inst_def456",
  "state": "running",
  "endpoints": {
    "ssh": "ssh://instance123.faas.io:22001",
    "http": {
      "webapp": "https://webapp-inst456.faas.io"
    }
  },
  "resources": {
    "cpu_cores": 2,
    "memory_mb": 4096
  },
  "created_at": "2024-01-15T10:40:00Z",
  "updated_at": "2024-01-15T10:41:00Z"
}
```

### List Instances

Get all instances for your account.

```http
GET /instances
```

#### Query Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| state | string | Filter by state (pending, running, stopped) |
| limit | integer | Number of results |
| offset | integer | Pagination offset |

### Stop Instance

Stop a running instance.

```http
POST /instances/{instance_id}/stop
```

### Pause Instance

Pause an instance (checkpoint and suspend).

```http
POST /instances/{instance_id}/pause
```

### Resume Instance

Resume a paused instance.

```http
POST /instances/{instance_id}/resume
```

## SSH & Port Forwarding

### Get SSH Info

Get SSH credentials for an instance.

```http
GET /instances/{instance_id}/ssh
```

#### Response

```json
{
  "host": "instance123.faas.io",
  "port": 22001,
  "username": "root",
  "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\n...",
  "fingerprint": "SHA256:xyzabc123..."
}
```

### Expose Service

Expose an HTTP service from instance.

```http
POST /instances/{instance_id}/expose
```

#### Request Body

```json
{
  "name": "webapp",
  "port": 3000
}
```

#### Response

```json
{
  "url": "https://webapp-inst456.faas.io"
}
```

### Hide Service

Remove exposed service.

```http
POST /instances/{instance_id}/unexpose
```

#### Request Body

```json
{
  "name": "webapp"
}
```

### Create Port Forward

Set up port forwarding.

```http
POST /instances/{instance_id}/port-forward
```

#### Request Body

```json
{
  "local_port": 8080,
  "remote_port": 3000
}
```

## File Operations

### Copy Files

Copy files to/from instance.

```http
POST /instances/{instance_id}/copy
```

#### Request Body

```json
{
  "direction": "upload",
  "local_path": "/local/project",
  "remote_path": "/workspace"
}
```

### Sync Files

Bidirectional file sync.

```http
POST /instances/{instance_id}/sync
```

#### Request Body

```json
{
  "local_dir": "/local/project",
  "remote_dir": "/workspace",
  "exclude": ["node_modules", ".git"],
  "bidirectional": true
}
```

## Development Environments

### Launch VSCode

Start VSCode server in instance.

```http
POST /instances/{instance_id}/vscode
```

#### Response

```json
{
  "url": "https://vscode-inst456.faas.io",
  "type": "vscode"
}
```

### Launch Jupyter

Start Jupyter notebook server.

```http
POST /instances/{instance_id}/jupyter
```

#### Response

```json
{
  "url": "https://jupyter-inst456.faas.io",
  "type": "jupyter",
  "token": "abc123..."
}
```

### Launch VNC Desktop

Start VNC remote desktop.

```http
POST /instances/{instance_id}/vnc
```

#### Response

```json
{
  "url": "https://vnc-inst456.faas.io",
  "type": "vnc",
  "password": "xyz789"
}
```

## Usage & Billing

### Get Usage Metrics

Get detailed usage for a time period.

```http
GET /usage
```

#### Query Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| start_date | ISO 8601 | Start of period |
| end_date | ISO 8601 | End of period |

#### Response

```json
{
  "period_start": "2024-01-01T00:00:00Z",
  "period_end": "2024-01-31T23:59:59Z",
  "executions": {
    "total_count": 10000,
    "successful_count": 9950,
    "failed_count": 50,
    "cpu_seconds": 50000,
    "memory_gb_seconds": 25000,
    "by_mode": {
      "ephemeral": 5000,
      "cached": 3000,
      "checkpointed": 1500,
      "branched": 500
    }
  },
  "storage": {
    "snapshots_created": 100,
    "total_gb_stored": 50.5
  },
  "network": {
    "ingress_gb": 10.5,
    "egress_gb": 25.3
  },
  "costs": {
    "compute_cost": 12.50,
    "storage_cost": 5.05,
    "network_cost": 2.28,
    "total_cost": 19.83,
    "credits_remaining": 80.17
  }
}
```

### Get Current Usage

Get real-time usage status.

```http
GET /usage/current
```

#### Response

```json
{
  "current_balance": 80.17,
  "pending_charges": 0.45,
  "rate_limit_remaining": 85,
  "storage_used_bytes": 53687091200,
  "active_executions": 3
}
```

## Error Responses

All errors follow this format:

```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Rate limit exceeded. Please retry after 60 seconds.",
    "details": {
      "limit": 100,
      "remaining": 0,
      "reset_at": "2024-01-15T11:00:00Z"
    }
  }
}
```

### Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| UNAUTHORIZED | 401 | Invalid or missing API key |
| FORBIDDEN | 403 | Insufficient permissions |
| NOT_FOUND | 404 | Resource not found |
| RATE_LIMIT_EXCEEDED | 429 | Rate limit exceeded |
| INVALID_REQUEST | 400 | Invalid request parameters |
| EXECUTION_TIMEOUT | 408 | Execution timed out |
| INSUFFICIENT_RESOURCES | 507 | Not enough resources |
| PAYMENT_REQUIRED | 402 | Insufficient balance |
| INTERNAL_ERROR | 500 | Internal server error |

## WebSocket Events

### Output Stream Events

#### Subscribe
```json
{
  "type": "subscribe",
  "execution_id": "exec_123"
}
```

#### Unsubscribe
```json
{
  "type": "unsubscribe",
  "execution_id": "exec_123"
}
```

#### Input
```json
{
  "type": "input",
  "execution_id": "exec_123",
  "data": "user input\n"
}
```

#### Resize Terminal
```json
{
  "type": "resize",
  "execution_id": "exec_123",
  "cols": 120,
  "rows": 40
}
```

### Stream Messages

#### Output
```json
{
  "type": "output",
  "execution_id": "exec_123",
  "stream": "stdout",
  "data": "Program output\n"
}
```

#### Error
```json
{
  "type": "error",
  "execution_id": "exec_123",
  "error": "Execution failed"
}
```

#### Progress
```json
{
  "type": "progress",
  "execution_id": "exec_123",
  "progress": 0.75,
  "message": "Processing..."
}
```

#### Completed
```json
{
  "type": "completed",
  "execution_id": "exec_123",
  "exit_code": 0
}
```

## SDK Examples

### TypeScript

```typescript
import { FaaSClient } from '@faas/sdk';

const client = new FaaSClient({
  apiKey: process.env.FAAS_API_KEY,
  endpoint: 'https://api.faas.platform'
});

// Execute with streaming
const execution = await client.execute({
  code: 'for i in range(10): print(i)',
  language: 'python',
  mode: 'cached'
});

// Stream output
const stream = client.stream(execution.id);
stream.on('output', (data) => console.log(data));
stream.on('complete', (exitCode) => console.log('Done:', exitCode));
```

### Python

```python
from faas_sdk import FaaSClient
import asyncio

client = FaaSClient(
    api_key=os.environ['FAAS_API_KEY']
)

# Execute and wait
result = client.execute(
    code="print('Hello World')",
    language="python"
)

# Async streaming
async def stream_output():
    async for output in client.stream(result.execution_id):
        print(output.data, end='')

asyncio.run(stream_output())
```

### cURL

```bash
# Execute code
curl -X POST https://api.faas.platform/api/v1/execute \
  -H "Authorization: Bearer your_api_key" \
  -H "Content-Type: application/json" \
  -d '{
    "code": "console.log(\"Hello World\")",
    "language": "javascript",
    "mode": "ephemeral"
  }'

# Create snapshot
curl -X POST https://api.faas.platform/api/v1/snapshots \
  -H "Authorization: Bearer your_api_key" \
  -H "Content-Type: application/json" \
  -d '{
    "execution_id": "exec_123"
  }'
```

## Tangle/Polkadot Integration

Submit jobs to Tangle for blockchain-verified execution:

```typescript
import { TangleFaaSClient } from '@faas/sdk';

const client = new TangleFaaSClient({
  rpcUrl: 'wss://tangle.network',
  blueprintId: 123
});

// Submit job on-chain
const jobId = await client.submitJob({
  code: 'console.log("Verified execution")',
  language: 'javascript'
});

// Wait for result
const result = await client.waitForResult(jobId);
console.log('Verified result:', result);
```