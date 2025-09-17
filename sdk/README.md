# FaaS Platform SDKs

Official SDKs for the FaaS multi-mode execution platform.

## Available SDKs

- **TypeScript/JavaScript** - Full-featured SDK with TypeScript support
- **Python** - Async-first Python SDK with streaming support
- **Rust** - Native Rust integration (via faas-lib)

## Installation

### TypeScript/JavaScript

```bash
npm install @faas/sdk
# or
yarn add @faas/sdk
```

### Python

```bash
pip install faas-sdk
```

## Quick Start

### TypeScript

```typescript
import { FaaSClient } from '@faas/sdk';

const client = new FaaSClient({
  apiKey: 'your-api-key'
});

// Simple execution
const result = await client.execute({
  code: 'console.log("Hello World")',
  language: 'javascript'
});

console.log(result.stdout);
```

### Python

```python
from faas_sdk import FaaSClient

client = FaaSClient(api_key='your-api-key')

# Simple execution
result = client.execute(
    code='print("Hello World")',
    language='python'
)

print(result.stdout)
```

## Execution Modes

### Ephemeral Mode

Stateless execution with fresh environment.

```typescript
const result = await client.execute({
  code: 'console.log("Stateless")',
  language: 'javascript',
  mode: 'ephemeral'
});
```

### Cached Mode

Reuses warm containers for faster execution.

```typescript
const result = await client.execute({
  code: 'console.log("Fast start")',
  language: 'javascript',
  mode: 'cached'
});
// Subsequent calls with same config will be faster
```

### Checkpointed Mode

Creates snapshots for instant restoration.

```typescript
// Execute with checkpointing
const result = await client.execute({
  code: `
    // Heavy initialization
    const data = loadLargeDataset();
    processData(data);
  `,
  language: 'javascript',
  mode: 'checkpointed'
});

// Create snapshot after execution
const snapshot = await client.createSnapshot(result.executionId);

// Restore from snapshot (instant)
const restored = await client.restoreSnapshot(snapshot.id);
```

### Branched Mode

Sub-250ms branching for parallel exploration.

```typescript
// Create initial snapshot
const snapshot = await client.createSnapshot(executionId);

// Branch 1
const branch1 = await client.createBranch(snapshot.id);
await client.execute({
  executionId: branch1.executionId,
  code: 'explorePathA()',
  mode: 'branched'
});

// Branch 2 (parallel exploration)
const branch2 = await client.createBranch(snapshot.id);
await client.execute({
  executionId: branch2.executionId,
  code: 'explorePathB()',
  mode: 'branched'
});

// Merge branches
const merged = await client.mergeBranches([branch1.id, branch2.id]);
```

### Persistent Mode

Long-running instances with SSH access.

```typescript
// Start persistent instance
const instance = await client.instances.start({
  snapshotId: snapshot.id,
  resources: {
    cpuCores: 2,
    memoryMb: 4096,
    diskGb: 20
  }
});

// Get SSH credentials
const ssh = await client.instances.getSshInfo(instance.id);
console.log(`SSH: ssh -i key.pem ${ssh.username}@${ssh.host}:${ssh.port}`);

// Expose HTTP service
const url = await client.instances.expose(instance.id, {
  name: 'webapp',
  port: 3000
});
console.log(`Web app: ${url}`);
```

## Real-time Streaming

### TypeScript

```typescript
// Stream execution output
const stream = client.stream(executionId);

stream.on('output', (data) => {
  console.log('Output:', data);
});

stream.on('error', (error) => {
  console.error('Error:', error);
});

stream.on('progress', (progress) => {
  console.log(`Progress: ${progress.percent}% - ${progress.message}`);
});

stream.on('complete', (exitCode) => {
  console.log('Completed with exit code:', exitCode);
});

// Send input to running execution
stream.sendInput('user input\n');

// Resize terminal
stream.resize(120, 40);
```

### Python

```python
import asyncio

async def stream_output():
    async for event in client.stream(execution_id):
        if event.type == 'output':
            print(event.data, end='')
        elif event.type == 'error':
            print(f"Error: {event.error}")
        elif event.type == 'complete':
            print(f"Exit code: {event.exit_code}")

asyncio.run(stream_output())
```

## Instance Management

### Start Instance

```typescript
const instance = await client.instances.start({
  snapshotId: 'snap_123',
  resources: {
    cpuCores: 4,
    memoryMb: 8192,
    diskGb: 50,
    gpuCount: 1
  },
  ttl: 3600, // 1 hour
  autoStop: true
});
```

### Manage Lifecycle

```typescript
// Pause instance (checkpoint state)
await client.instances.pause(instanceId);

// Resume instance
await client.instances.resume(instanceId);

// Stop instance
await client.instances.stop(instanceId);
```

### Development Environments

```typescript
// Launch VSCode server
const vscode = await client.instances.launchVSCode(instanceId);
console.log(`VSCode: ${vscode.url}`);

// Launch Jupyter notebook
const jupyter = await client.instances.launchJupyter(instanceId);
console.log(`Jupyter: ${jupyter.url}`);
console.log(`Token: ${jupyter.token}`);

// Launch VNC desktop
const vnc = await client.instances.launchVNC(instanceId);
console.log(`VNC: ${vnc.url}`);
console.log(`Password: ${vnc.password}`);
```

### File Operations

```typescript
// Copy files to instance
await client.instances.copyFiles(instanceId, {
  direction: 'upload',
  localPath: './project',
  remotePath: '/workspace'
});

// Sync files bidirectionally
await client.instances.syncFiles(instanceId, {
  localDir: './project',
  remoteDir: '/workspace',
  exclude: ['node_modules', '.git'],
  bidirectional: true
});
```

### Port Forwarding

```typescript
// Expose HTTP service
const url = await client.instances.expose(instanceId, {
  name: 'api',
  port: 8080
});

// Create port forward
const forward = await client.instances.createPortForward(instanceId, {
  localPort: 8080,
  remotePort: 3000
});
```

## Tangle/Polkadot Integration

Submit jobs to Tangle blockchain for verifiable execution.

### TypeScript

```typescript
import { TangleFaaSClient } from '@faas/sdk';

const tangleClient = new TangleFaaSClient({
  rpcUrl: 'wss://tangle.network',
  blueprintId: 123,
  account: keyringPair
});

// Submit job on-chain
const jobId = await tangleClient.submitJob({
  code: 'console.log("Blockchain verified")',
  language: 'javascript',
  mode: 'ephemeral'
});

// Monitor job status
tangleClient.on('jobUpdate', (update) => {
  console.log(`Job ${update.jobId}: ${update.status}`);
});

// Wait for verified result
const result = await tangleClient.waitForResult(jobId);
console.log('Verified output:', result.stdout);
```

### Python

```python
from faas_sdk import TangleFaaSClient

tangle_client = TangleFaaSClient(
    rpc_url='wss://tangle.network',
    blueprint_id=123,
    account=account
)

# Submit job
job_id = await tangle_client.submit_job(
    code='print("Blockchain verified")',
    language='python'
)

# Wait for result
result = await tangle_client.wait_for_result(job_id)
print(f"Verified: {result.stdout}")
```

## Advanced Usage

### Batch Execution

```typescript
// Execute multiple jobs in parallel
const jobs = [
  { code: 'console.log("Job 1")', language: 'javascript' },
  { code: 'print("Job 2")', language: 'python' },
  { code: 'fmt.Println("Job 3")', language: 'go' }
];

const results = await Promise.all(
  jobs.map(job => client.execute(job))
);
```

### Custom Resources

```typescript
const result = await client.execute({
  code: 'heavy_computation()',
  language: 'python',
  resources: {
    cpuCores: 8,
    memoryMb: 16384,
    timeoutMs: 300000, // 5 minutes
    gpuCount: 2
  }
});
```

### Environment Variables

```typescript
const result = await client.execute({
  code: 'console.log(process.env.MY_VAR)',
  language: 'javascript',
  envVars: {
    MY_VAR: 'secret_value',
    NODE_ENV: 'production'
  }
});
```

### Input Files

```typescript
const result = await client.execute({
  code: `
    const fs = require('fs');
    const data = fs.readFileSync('input.json');
    console.log(JSON.parse(data));
  `,
  language: 'javascript',
  files: {
    'input.json': Buffer.from(JSON.stringify({ key: 'value' }))
  }
});
```

## Error Handling

### TypeScript

```typescript
try {
  const result = await client.execute({
    code: 'invalid syntax',
    language: 'python'
  });
} catch (error) {
  if (error.code === 'EXECUTION_ERROR') {
    console.error('Execution failed:', error.message);
    console.error('Stderr:', error.stderr);
  } else if (error.code === 'RATE_LIMIT_EXCEEDED') {
    console.error('Rate limit hit, retry after:', error.resetAt);
  }
}
```

### Python

```python
from faas_sdk.exceptions import ExecutionError, RateLimitError

try:
    result = client.execute(
        code='invalid syntax',
        language='python'
    )
except ExecutionError as e:
    print(f"Execution failed: {e.message}")
    print(f"Stderr: {e.stderr}")
except RateLimitError as e:
    print(f"Rate limit exceeded, retry after: {e.reset_at}")
```

## Usage Tracking

### Get Usage Metrics

```typescript
// Get usage for current month
const usage = await client.usage.getMetrics();
console.log(`Executions: ${usage.executions.totalCount}`);
console.log(`CPU seconds: ${usage.executions.cpuSeconds}`);
console.log(`Total cost: $${usage.costs.totalCost}`);

// Get usage for specific period
const historicalUsage = await client.usage.getMetrics({
  startDate: '2024-01-01',
  endDate: '2024-01-31'
});
```

### Monitor Current Usage

```typescript
// Get real-time usage
const current = await client.usage.getCurrent();
console.log(`Balance: $${current.currentBalance}`);
console.log(`Rate limit remaining: ${current.rateLimitRemaining}`);
console.log(`Active executions: ${current.activeExecutions}`);
```

## Configuration

### TypeScript

```typescript
const client = new FaaSClient({
  apiKey: 'your-api-key',
  endpoint: 'https://api.faas.platform', // Optional custom endpoint
  timeout: 30000, // Request timeout in ms
  maxRetries: 3, // Automatic retry on failure
  debug: true // Enable debug logging
});
```

### Python

```python
client = FaaSClient(
    api_key='your-api-key',
    endpoint='https://api.faas.platform',
    timeout=30,
    max_retries=3,
    debug=True
)
```

## Best Practices

1. **Use appropriate execution modes**
   - Ephemeral for stateless functions
   - Cached for frequently called functions
   - Checkpointed for heavy initialization
   - Branched for exploration/testing
   - Persistent for development environments

2. **Implement proper error handling**
   - Always catch and handle execution errors
   - Implement exponential backoff for rate limits
   - Log errors for debugging

3. **Optimize resource usage**
   - Start with minimal resources
   - Monitor usage and adjust as needed
   - Use caching to reduce cold starts

4. **Security considerations**
   - Never hardcode API keys
   - Use environment variables for secrets
   - Validate all user input

5. **Performance optimization**
   - Batch operations when possible
   - Use streaming for long-running tasks
   - Leverage snapshots for quick restoration

## Support

- Documentation: https://docs.faas.platform
- API Reference: https://api.faas.platform/docs
- GitHub Issues: https://github.com/faas-platform/sdk/issues
- Discord: https://discord.gg/faas-platform