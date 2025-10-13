# FaaS Tangle SDK (TypeScript)

TypeScript client for submitting jobs to the FaaS Blueprint on Tangle Network.

## Installation

```bash
npm install @faas-platform/tangle-sdk
```

## Requirements

- Node.js 16+
- TypeScript 4.5+
- Polkadot.js API dependencies (installed automatically)

## Quick Start

```typescript
import { TangleClient } from '@faas-platform/tangle-sdk';

async function main() {
  // Connect to Tangle network
  const client = await TangleClient.connect('ws://localhost:9944');

  // Submit Job 0: Execute Function
  const result = await client.executeFunction({
    image: 'alpine:latest',
    command: ['echo', 'Hello from blockchain!'],
    envVars: null,
    payload: Buffer.from([])
  });

  console.log('Job call ID:', result.callId);
  console.log('Output:', Buffer.from(result.result).toString());
}
```

## Available Operations

### Job 0: Execute Function
```typescript
const result = await client.executeFunction({
  image: 'alpine:latest',
  command: ['echo', 'test'],
  envVars: ['KEY=value'],
  payload: Buffer.from([])
});
```

### Job 1: Execute Advanced
```typescript
const result = await client.executeAdvanced({
  image: 'rust:latest',
  command: ['cargo', 'build'],
  mode: 'cached',
  checkpointId: null,
  branchFrom: null,
  timeoutSecs: 60
});
```

### Job 2-11: Other Job Types
See full documentation for snapshot, instance, port, and file operations.

## Query Results

```typescript
// Get job result
const result = await client.getJobResult(serviceId, callId);

// Check assigned operator
const operator = await client.getAssignedOperator(callId);
```

## Architecture

```
TypeScript Client
     ↓ (Polkadot.js API)
Tangle Network (Substrate)
     ↓ (Smart Contract Call)
FaaSBlueprint Contract
     ↓ (Job Assignment)
Decentralized Operators
```

## Development Status

**Current**: Structure and types defined, implementation pending
**Next**: Full Polkadot.js integration for contract interaction

## Related Packages

- `@faas-platform/sdk` - HTTP client for gateway server
- `@faas-platform/tangle-sdk` - Blockchain client (this package)
