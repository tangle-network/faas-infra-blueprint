# FaaS Platform TypeScript/JavaScript SDK

[![npm](https://img.shields.io/npm/v/@faas-platform/sdk.svg)](https://www.npmjs.com/package/@faas-platform/sdk)
[![Documentation](https://img.shields.io/badge/docs-latest-brightgreen.svg)](https://docs.faas-platform.com/typescript-sdk)

Official TypeScript/JavaScript SDK for the FaaS Platform with full type safety and event-driven architecture.

## Features

- 🚀 **Dual Runtime Support**: Docker containers and Firecracker microVMs
- 📊 **Intelligent Caching**: Automatic result caching with configurable TTL
- 🔥 **Pre-warming**: Zero cold starts with warm container pools
- 🌳 **Execution Forking**: Branch workflows for A/B testing
- 📈 **Auto-scaling**: Predictive scaling based on load patterns
- 📋 **Rich Events**: Event-driven architecture with detailed monitoring
- 🔄 **Method Chaining**: Fluent API for easy configuration
- ⚡ **Full TypeScript**: Complete type safety and IntelliSense support

## Installation

```bash
npm install @faas-platform/sdk
```

For TypeScript projects:
```bash
npm install @faas-platform/sdk @types/node
```

## Quick Start

### JavaScript (Node.js)

```javascript
const { FaaSClient } = require('@faas-platform/sdk');

async function main() {
  const client = new FaaSClient('http://localhost:8080');

  // Simple execution
  const result = await client.runJavaScript('console.log("Hello, World!")');
  console.log(result.output); // Output: Hello, World!
}

main().catch(console.error);
```

### TypeScript

```typescript
import { FaaSClient, Runtime, ExecutionMode } from '@faas-platform/sdk';

async function main(): Promise<void> {
  const client = new FaaSClient('http://localhost:8080');

  // Method chaining for configuration
  const result = await client
    .useFirecracker()
    .setCaching(true)
    .execute({
      command: 'node process.js',
      image: 'node:20-alpine'
    });

  console.log(`Execution completed in ${result.durationMs}ms`);
}
```

## API Reference

### FaaSClient

The main client class extending EventEmitter for event-driven operations.

#### Constructor

```typescript
new FaaSClient(config: ClientConfig | string)
```

#### Methods

- `runJavaScript(code: string)` - Execute JavaScript/Node.js code
- `runTypeScript(code: string)` - Execute TypeScript code
- `runPython(code: string)` - Execute Python code
- `runBash(script: string)` - Execute bash scripts
- `execute(options: ExecuteOptions)` - General-purpose execution
- `executeAdvanced(request: AdvancedExecuteRequest)` - Advanced execution
- `forkExecution(parentId: string, command: string)` - Fork existing execution
- `prewarm(image: string, count: number)` - Pre-warm containers
- `getMetrics()` - Get server performance metrics
- `getClientMetrics()` - Get client-side metrics
- `healthCheck()` - Check platform health

#### Method Chaining

```typescript
const client = new FaaSClient('http://localhost:8080')
  .useFirecracker()     // Set runtime to Firecracker
  .setCaching(true)     // Enable caching
  .setRetries(5)        // Set retry attempts
  .setTimeout(60000);   // Set timeout
```

### Events

The client emits the following events:

```typescript
client.on('execution', (event) => {
  console.log(`Execution completed in ${event.elapsedMs}ms`);
});

client.on('retry', (event) => {
  console.log(`Retry attempt ${event.attempt}: ${event.error}`);
});

client.on('error', (error) => {
  console.error('Client error:', error);
});

client.on('cache-hit', (event) => {
  console.log('Cache hit for key:', event.cacheKey);
});
```

### Runtime Selection

```typescript
import { Runtime } from '@faas-platform/sdk';

// Development with Docker
const devClient = new FaaSClient('http://localhost:8080', {
  runtime: Runtime.Docker
});

// Production with Firecracker
const prodClient = new FaaSClient('https://api.example.com', {
  runtime: Runtime.Firecracker
});

// Automatic selection
const smartClient = new FaaSClient('http://localhost:8080', {
  runtime: Runtime.Auto
});
```

### Execution Modes

```typescript
import { ExecutionMode } from '@faas-platform/sdk';

// Cached execution
const result = await client.executeAdvanced({
  command: 'node inference.js',
  mode: ExecutionMode.Cached,
  image: 'node:20-alpine'
});

// Persistent service
const service = await client.executeAdvanced({
  command: 'node server.js',
  mode: ExecutionMode.Persistent,
  image: 'node:20-alpine'
});
```

## Examples

See the [examples directory](../../examples/typescript/) for complete examples:

- [quickstart.ts](../../examples/typescript/quickstart.ts) - Basic usage patterns and features

## Advanced Usage

### Concurrent Execution

```typescript
const tasks = Array.from({ length: 10 }, (_, i) =>
  client.runJavaScript(`console.log('Task ${i} completed')`)
);

const results = await Promise.all(tasks);
console.log(`Completed ${results.length} executions`);
```

### Error Handling with Types

```typescript
import axios from 'axios';

try {
  const result = await client.execute({ command: 'invalid-command' });
} catch (error) {
  if (axios.isAxiosError(error)) {
    console.error('Network error:', error.message);
  } else {
    console.error('Execution error:', error);
  }
}
```

### Performance Monitoring

```typescript
// Monitor performance metrics
setInterval(async () => {
  const serverMetrics = await client.getMetrics();
  const clientMetrics = client.getClientMetrics();

  console.log('Server metrics:', serverMetrics);
  console.log('Client cache hit rate:', clientMetrics.cacheHitRate);
}, 30000);
```

## Performance Tips

1. **Use method chaining** to avoid creating multiple instances
2. **Enable caching** for deterministic computations
3. **Pre-warm containers** for critical paths
4. **Use Firecracker** for production workloads requiring isolation
5. **Monitor events** for performance insights
6. **Batch operations** when possible

## TypeScript Configuration

For optimal TypeScript support, configure your `tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "lib": ["ES2020"],
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true
  }
}
```

## License

This project is licensed under the MIT License.