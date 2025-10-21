# FaaS Platform TypeScript SDK

Official TypeScript/JavaScript SDK for the FaaS Platform. Execute code in Docker containers or Firecracker microVMs with full type safety.

## Installation

```bash
npm install @faas-platform/sdk
```

TypeScript projects also need:
```bash
npm install -D @types/node
```

## Prerequisites

- FaaS Gateway server running (default: `http://localhost:8080`)
- Docker available on the gateway host
- Node.js ≥16.0.0

## Quick Start

### JavaScript

```javascript
const { FaaSClient } = require('@faas-platform/sdk');

async function main() {
  const client = new FaaSClient('http://localhost:8080');

  const result = await client.runJavaScript('console.log("Hello, World!")');
  console.log(result.output); // Hello, World!
  console.log(result.exitCode); // 0
  console.log(result.durationMs); // 245
}

main().catch(console.error);
```

### TypeScript

```typescript
import { FaaSClient, Runtime, ExecutionResult } from '@faas-platform/sdk';

async function main(): Promise<void> {
  const client = new FaaSClient('http://localhost:8080');

  const result: ExecutionResult = await client
    .useFirecracker()
    .setCaching(true)
    .execute({
      command: 'node process.js',
      image: 'node:20-alpine'
    });

  console.log(`Completed in ${result.durationMs}ms`);
}
```

## Core API

### Client Constructor

```typescript
// Simple configuration
const client = new FaaSClient('http://localhost:8080');

// Full configuration
const client = new FaaSClient({
  baseUrl: 'http://localhost:8080',
  runtime: Runtime.Docker,
  cacheEnabled: true,
  maxRetries: 3,
  timeout: 30000,
  apiKey: process.env.FAAS_API_KEY
});
```

### Execution Methods

#### `execute(options)`

General-purpose execution with full control.

```typescript
const result = await client.execute({
  command: 'python script.py',
  image: 'python:3.11-slim',
  runtime: Runtime.Docker,
  envVars: { API_KEY: 'secret' },
  workingDir: '/app',
  timeoutMs: 60000,
  cacheKey: 'optional-cache-key'
});
```

**Returns:** `ExecutionResult`
- `requestId: string` - Unique execution identifier
- `output?: string` - Combined stdout/stderr output
- `logs?: string` - Stderr only
- `error?: string` - Error message if execution failed
- `exitCode?: number` - Process exit code
- `durationMs: number` - Execution time in milliseconds
- `cacheHit: boolean` - Whether result was served from cache
- `runtimeUsed?: Runtime` - Runtime that executed the request

#### `runPython(code)`

Execute Python code directly.

```typescript
const result = await client.runPython(`
data = [1, 2, 3, 4, 5]
print(f"Sum: {sum(data)}")
`);
```

**Image:** `python:3.11-slim`

#### `runJavaScript(code)`

Execute JavaScript/Node.js code directly.

```typescript
const result = await client.runJavaScript(`
const data = [1, 2, 3, 4, 5];
const sum = data.reduce((a, b) => a + b, 0);
console.log("Sum: " + sum);
`);
```

**Image:** `node:20-slim`

#### `runTypeScript(code)`

Execute TypeScript code (transpiled via ts-node).

```typescript
const result = await client.runTypeScript(`
const greet = (name: string): string => \`Hello, \${name}\`;
console.log(greet("World"));
`);
```

**Image:** `node:20-slim`

#### `runBash(script)`

Execute bash scripts.

```typescript
const result = await client.runBash(`
echo "content" > /tmp/test.txt
cat /tmp/test.txt
wc -c /tmp/test.txt
`);
```

**Image:** `alpine:latest`

### Runtime Selection

```typescript
import { Runtime } from '@faas-platform/sdk';

// Set runtime globally
const client = new FaaSClient({
  baseUrl: 'http://localhost:8080',
  runtime: Runtime.Docker
});

// Override per request
await client.execute({
  command: 'echo test',
  runtime: Runtime.Firecracker
});

// Method chaining
await client.useDocker().execute({ command: 'echo test' });
await client.useFirecracker().execute({ command: 'echo test' });
```

**Available Runtimes:**
- `Runtime.Docker` - Docker containers (50-200ms cold start)
- `Runtime.Firecracker` - Firecracker microVMs (~125ms cold start, hardware isolation)
- `Runtime.Auto` - Platform selects optimal runtime

### Method Chaining

Configure client behavior fluently.

```typescript
const result = await client
  .useDocker()
  .setCaching(false)
  .execute({ command: 'echo test' });
```

**Available Methods:**
- `useDocker()` - Set runtime to Docker
- `useFirecracker()` - Set runtime to Firecracker
- `setCaching(enabled: boolean)` - Enable/disable result caching

### Events

Client emits events for monitoring and debugging.

```typescript
client.on('execution', (event) => {
  console.log(`Completed in ${event.elapsedMs}ms`);
  console.log(`Cache hit: ${event.cacheHit}`);
  console.log(`Result:`, event.result);
});

client.on('retry', (event) => {
  console.log(`Retry ${event.attempt}: ${event.error}`);
});
```

**Event Types:**
- `execution` - Emitted after each execution completes
  - `result: any` - Raw API response
  - `elapsedMs: number` - Client-side elapsed time
  - `cacheHit: boolean` - Cache status
- `retry` - Emitted when request is retried
  - `attempt: number` - Retry attempt number (0-indexed)
  - `error: Error` - Error that triggered retry

### Utility Methods

#### `healthCheck()`

Check gateway health status.

```typescript
const health = await client.healthCheck();
console.log(health.status); // "ok"
```

#### `getMetrics()`

Get server-side performance metrics.

```typescript
const metrics = await client.getMetrics();
console.log(metrics);
```

#### `getClientMetrics()`

Get client-side metrics (synchronous).

```typescript
const metrics = client.getClientMetrics();
console.log(`Total requests: ${metrics.totalRequests}`);
console.log(`Cache hit rate: ${metrics.cacheHitRate}`);
console.log(`Average latency: ${metrics.averageLatencyMs}ms`);
console.log(`Error rate: ${metrics.errorRate}`);
```

#### `getCacheKey(content)`

Generate deterministic MD5 cache key.

```typescript
const key = client.getCacheKey('print("hello")');
console.log(key); // "5d41402abc4b2a76b9719d911017c592"
```

#### `prewarm(image, count)`

Pre-warm container pool to reduce cold starts.

```typescript
await client.prewarm('python:3.11-slim', 5);
```

## Advanced Features

### Concurrent Execution

```typescript
const tasks = Array(10).fill(null).map((_, i) =>
  client.execute({
    command: `echo "Task ${i}"`,
    image: 'alpine:latest'
  })
);

const results = await Promise.all(tasks);
console.log(`Completed ${results.length} executions`);
```

### Error Handling

```typescript
import axios from 'axios';

try {
  const result = await client.execute({
    command: 'exit 1',
    image: 'alpine:latest'
  });

  if (result.exitCode !== 0) {
    console.error('Command failed:', result.error);
  }
} catch (error) {
  if (axios.isAxiosError(error)) {
    console.error('Network error:', error.message);
  } else {
    console.error('Unexpected error:', error);
  }
}
```

### Environment Variables

```typescript
const result = await client.execute({
  command: 'python app.py',
  image: 'python:3.11-slim',
  envVars: {
    DATABASE_URL: 'postgres://...',
    API_KEY: process.env.API_KEY,
    NODE_ENV: 'production'
  }
});
```

### Custom Images

```typescript
const result = await client.execute({
  command: 'cargo run --release',
  image: 'rust:1.75-alpine',
  timeoutMs: 300000 // 5 minutes
});
```

### Execution Forking

Fork parent execution for A/B testing.

```typescript
const parent = await client.execute({
  command: 'node setup.js',
  image: 'node:20-alpine'
});

const variants = await Promise.all([
  client.forkExecution(parent.requestId, 'node variant-a.js'),
  client.forkExecution(parent.requestId, 'node variant-b.js')
]);
```

### Snapshots

Create execution snapshots for state management.

```typescript
const snapshot = await client.createSnapshot(
  containerId,
  'checkpoint-1',
  'State after initialization'
);
```

## Performance

### Optimization Strategies

1. **Enable caching** for deterministic workloads - reduces latency to <10ms
2. **Pre-warm containers** for critical paths - eliminates cold start overhead
3. **Use method chaining** - avoids creating multiple client instances
4. **Batch concurrent executions** - maximizes throughput
5. **Select appropriate runtime** - Docker for dev, Firecracker for production

### Cache Behavior

- Automatic cache key generation from `command` + `image`
- Custom cache keys via `cacheKey` parameter
- Cache detection via `cacheHit` field (<10ms responses)
- Disable caching: `setCaching(false)`

### Retry Logic

- Default: 3 retries with exponential backoff
- Configurable via `maxRetries` in constructor
- Backoff: 100ms × 2^attempt
- Emits `retry` event on each attempt

## TypeScript Configuration

Recommended `tsconfig.json` for optimal type safety:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "lib": ["ES2020"],
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "resolveJsonModule": true
  }
}
```

## Testing

Integration tests require a running gateway:

```bash
# Terminal 1: Start gateway
cargo run --package faas-gateway-server

# Terminal 2: Run tests
npm test
```

Set custom gateway URL:
```bash
FAAS_GATEWAY_URL=http://localhost:9090 npm test
```

## License

MIT
