# TypeScript SDK Implementation

## Package Structure

```
sdk/typescript/
├── src/
│   ├── index.ts
│   ├── client.ts
│   ├── snapshots.ts
│   ├── instances.ts
│   ├── branches.ts
│   ├── execution.ts
│   ├── files.ts
│   ├── network.ts
│   ├── ssh.ts
│   ├── errors.ts
│   ├── types.ts
│   └── utils.ts
├── tests/
├── examples/
├── package.json
├── tsconfig.json
└── README.md
```

## Core Types

```typescript
// src/types.ts

export interface ClientConfig {
  apiKey?: string;
  endpoint?: string;
  timeout?: number;
  maxRetries?: number;
}

export interface ResourceConfig {
  vcpus: number;
  memory: number;
  diskSize: number;
  gpus?: GPUConfig[];
}

export interface SnapshotMetadata {
  name?: string;
  description?: string;
  tags?: Record<string, string>;
  environment?: Record<string, string>;
}

export enum InstanceState {
  PENDING = "pending",
  RUNNING = "running",
  PAUSED = "paused",
  STOPPED = "stopped",
  TERMINATED = "terminated"
}

export interface ExecutionResult {
  exitCode: number;
  stdout: string;
  stderr: string;
  duration: number;
  snapshotId?: string;
}
```

## Core Client

```typescript
// src/client.ts

import { SnapshotManager } from './snapshots';
import { InstanceManager } from './instances';
import { BranchManager } from './branches';
import { ExecutionManager } from './execution';
import { FileManager } from './files';
import { NetworkManager } from './network';
import { ClientConfig } from './types';

export class FaaSClient {
  private apiKey: string;
  private endpoint: string;
  private timeout: number;
  private maxRetries: number;

  public snapshots: SnapshotManager;
  public instances: InstanceManager;
  public branches: BranchManager;
  public execution: ExecutionManager;
  public files: FileManager;
  public network: NetworkManager;

  constructor(config?: ClientConfig) {
    this.apiKey = config?.apiKey || process.env.FAAS_API_KEY || '';
    this.endpoint = config?.endpoint || 'https://api.faas.io';
    this.timeout = config?.timeout || 30000;
    this.maxRetries = config?.maxRetries || 3;

    this.snapshots = new SnapshotManager(this);
    this.instances = new InstanceManager(this);
    this.branches = new BranchManager(this);
    this.execution = new ExecutionManager(this);
    this.files = new FileManager(this);
    this.network = new NetworkManager(this);
  }

  async exec(command: string, target?: string | Instance | Snapshot): Promise<ExecutionResult> {
    return this.execution.exec(command, target);
  }

  async branch(snapshotId?: string): Promise<Branch> {
    return this.branches.create(snapshotId);
  }
}
```

## Snapshot Operations

```typescript
// src/snapshots.ts

import { SnapshotMetadata, ResourceConfig } from './types';

export class Snapshot {
  constructor(
    public id: string,
    public parentId: string | null,
    public metadata: SnapshotMetadata,
    public createdAt: Date,
    public size: number,
    public checksum: string,
    private manager: SnapshotManager
  ) {}

  async exec(command: string): Promise<Snapshot> {
    const result = await this.manager.client.execution.execWithSnapshot(this, command);
    return this.manager.get(result.snapshotId!);
  }

  async upload(localPath: string, remotePath: string): Promise<Snapshot> {
    await this.manager.client.files.upload(this, localPath, remotePath);
    return this.exec(`echo "File uploaded: ${remotePath}"`);
  }

  async download(remotePath: string, localPath: string): Promise<void> {
    await this.manager.client.files.download(this, remotePath, localPath);
  }

  async branch(name?: string): Promise<Branch> {
    return this.manager.client.branches.create(this.id, name);
  }
}

export class SnapshotManager {
  constructor(public client: FaaSClient) {}

  async create(config?: {
    baseImage?: string;
    vcpus?: number;
    memory?: number;
    diskSize?: number;
    metadata?: SnapshotMetadata;
  }): Promise<Snapshot> {
    // Implementation
  }

  async get(id: string): Promise<Snapshot> {
    // Implementation
  }

  async list(filter?: {
    parentId?: string;
    tags?: Record<string, string>;
  }): Promise<Snapshot[]> {
    // Implementation
  }

  async delete(id: string): Promise<void> {
    // Implementation
  }

  async *iterate(filter?: any): AsyncIterableIterator<Snapshot> {
    let page = 0;
    while (true) {
      const snapshots = await this.list({ ...filter, page });
      if (snapshots.length === 0) break;
      for (const snapshot of snapshots) {
        yield snapshot;
      }
      page++;
    }
  }
}
```

## Instance Management

```typescript
// src/instances.ts

import { InstanceState, ResourceConfig } from './types';
import { SSHClient } from './ssh';

export class Instance {
  constructor(
    public id: string,
    public snapshotId: string,
    public state: InstanceState,
    public endpoints: Record<string, string>,
    public resources: ResourceConfig,
    private manager: InstanceManager
  ) {}

  async ssh(): Promise<SSHClient> {
    const client = new SSHClient(this);
    await client.connect();
    return client;
  }

  async exec(command: string): Promise<ExecutionResult> {
    return this.manager.client.execution.exec(command, this);
  }

  async stop(): Promise<void> {
    return this.manager.stop(this.id);
  }

  async pause(): Promise<void> {
    return this.manager.pause(this.id);
  }

  async resume(): Promise<void> {
    return this.manager.resume(this.id);
  }

  async exposeHttpService(name: string, port: number): Promise<ServiceEndpoint> {
    return this.manager.client.network.exposeHttp(this, name, port);
  }
}

export class InstanceManager {
  constructor(public client: FaaSClient) {}

  async start(config: {
    snapshotId: string;
    autoStop?: boolean;
    ttl?: number;
    resources?: ResourceConfig;
  }): Promise<Instance> {
    // Implementation
  }

  async stop(id: string): Promise<void> {
    // Implementation
  }

  async pause(id: string): Promise<void> {
    // Implementation
  }

  async resume(id: string): Promise<void> {
    // Implementation
  }

  async waitUntilReady(id: string, timeout = 300000): Promise<void> {
    const startTime = Date.now();
    while (Date.now() - startTime < timeout) {
      const instance = await this.get(id);
      if (instance.state === InstanceState.RUNNING) {
        return;
      }
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
    throw new Error(`Instance ${id} did not become ready within ${timeout}ms`);
  }

  async get(id: string): Promise<Instance> {
    // Implementation
  }

  async list(filter?: {
    state?: InstanceState;
    snapshotId?: string;
  }): Promise<Instance[]> {
    // Implementation
  }
}
```

## Branching Operations

```typescript
// src/branches.ts

export class Branch {
  constructor(
    public id: string,
    public parentId: string,
    public name: string | null,
    public metadata: Record<string, any>,
    public divergencePoint: Snapshot,
    private manager: BranchManager
  ) {}

  async exec(command: string): Promise<Branch> {
    const instance = await this.checkout();
    await instance.exec(command);
    const snapshot = await this.manager.client.snapshots.create({
      baseImage: this.divergencePoint.id
    });
    return new Branch(
      `${this.id}-next`,
      this.id,
      this.name,
      this.metadata,
      snapshot,
      this.manager
    );
  }

  async merge(other: Branch): Promise<Snapshot> {
    return this.manager.merge([this.id, other.id]);
  }

  async checkout(): Promise<Instance> {
    return this.manager.checkout(this.id);
  }
}

export class BranchManager {
  constructor(public client: FaaSClient) {}

  async create(snapshotId?: string, name?: string): Promise<Branch> {
    // Implementation
  }

  async list(parentId?: string): Promise<Branch[]> {
    // Implementation
  }

  async merge(branchIds: string[]): Promise<Snapshot> {
    // Implementation
  }

  async checkout(branchId: string): Promise<Instance> {
    // Implementation
  }

  async parallelMap<T, R>(
    items: T[],
    fn: (item: T, instance: Instance) => Promise<R>
  ): Promise<R[]> {
    const branches = await Promise.all(
      items.map(() => this.create())
    );

    const instances = await Promise.all(
      branches.map(branch => branch.checkout())
    );

    const results = await Promise.all(
      items.map((item, i) => fn(item, instances[i]))
    );

    await Promise.all(
      instances.map(instance => instance.stop())
    );

    return results;
  }
}
```

## Execution Operations

```typescript
// src/execution.ts

import { ExecutionResult } from './types';

export class ExecutionManager {
  constructor(private client: FaaSClient) {}

  async exec(
    command: string,
    target?: string | Instance | Snapshot
  ): Promise<ExecutionResult> {
    // Implementation
  }

  async execBatch(
    commands: string[],
    target?: string | Instance | Snapshot
  ): Promise<ExecutionResult[]> {
    return Promise.all(commands.map(cmd => this.exec(cmd, target)));
  }

  async execScript(
    script: string,
    target?: string | Instance | Snapshot,
    interpreter = 'bash'
  ): Promise<ExecutionResult> {
    // Implementation
  }

  async execWithSnapshot(
    target: Instance | Snapshot,
    command: string
  ): Promise<Snapshot> {
    // Implementation
  }
}
```

## File Operations

```typescript
// src/files.ts

export interface FileInfo {
  path: string;
  size: number;
  modified: Date;
  isDirectory: boolean;
  permissions: string;
}

export class FileManager {
  constructor(private client: FaaSClient) {}

  async upload(
    target: string | Instance | Snapshot,
    localPath: string,
    remotePath: string
  ): Promise<void> {
    // Implementation
  }

  async download(
    target: string | Instance | Snapshot,
    remotePath: string,
    localPath: string
  ): Promise<void> {
    // Implementation
  }

  async list(
    target: string | Instance | Snapshot,
    path: string
  ): Promise<FileInfo[]> {
    // Implementation
  }

  async sync(
    target: string | Instance | Snapshot,
    localPath: string,
    remotePath: string,
    options?: { bidirectional?: boolean; exclude?: string[] }
  ): Promise<void> {
    // Implementation
  }
}
```

## SSH Operations

```typescript
// src/ssh.ts

import { Client } from 'ssh2';
import { ExecutionResult } from './types';

export class SSHClient {
  private client: Client;
  private connected = false;

  constructor(private instance: Instance) {
    this.client = new Client();
  }

  async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.client
        .on('ready', () => {
          this.connected = true;
          resolve();
        })
        .on('error', reject)
        .connect({
          host: this.instance.endpoints.ssh,
          username: 'root',
          privateKey: await this.getPrivateKey()
        });
    });
  }

  async execCommand(command: string): Promise<ExecutionResult> {
    // Implementation
  }

  async putFile(localPath: string, remotePath: string): Promise<void> {
    // Implementation
  }

  async getFile(remotePath: string, localPath: string): Promise<void> {
    // Implementation
  }

  async createShell(): Promise<InteractiveShell> {
    // Implementation
  }

  async disconnect(): Promise<void> {
    if (this.connected) {
      this.client.end();
      this.connected = false;
    }
  }
}
```

## Usage Examples

```typescript
import { FaaSClient } from 'faas-sdk';

// Initialize client
const client = new FaaSClient({
  apiKey: 'your-api-key'
});

// Simple execution
const result = await client.exec("echo 'Hello World'");
console.log(result.stdout);

// Snapshot chaining
const snapshot = await client.snapshots
  .create({ baseImage: 'ubuntu:22.04' })
  .then(s => s.exec('apt-get update'))
  .then(s => s.exec('apt-get install -y nodejs npm'))
  .then(s => s.upload('./package.json', '/app/package.json'))
  .then(s => s.exec('cd /app && npm install'));

// Instance management
const instance = await client.instances.start({
  snapshotId: snapshot.id
});

await client.instances.waitUntilReady(instance.id);

const execResult = await instance.exec('node app.js');
console.log(execResult.stdout);

await instance.stop();

// Parallel execution
const processData = async (dataFile: string, instance: Instance) => {
  await client.files.upload(instance, dataFile, `/data/${dataFile}`);
  return instance.exec(`node process.js /data/${dataFile}`);
};

const dataFiles = ['data1.json', 'data2.json', 'data3.json'];
const results = await client.branches.parallelMap(dataFiles, processData);

// SSH operations
const ssh = await instance.ssh();
try {
  await ssh.putFile('./script.js', '/tmp/script.js');
  const result = await ssh.execCommand('node /tmp/script.js');
  await ssh.getFile('/tmp/output.json', './output.json');
} finally {
  await ssh.disconnect();
}

// Async iteration
for await (const snapshot of client.snapshots.iterate()) {
  console.log(snapshot.id, snapshot.metadata);
}
```