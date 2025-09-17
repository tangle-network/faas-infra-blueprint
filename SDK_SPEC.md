# SDK Specification

## Overview

Multi-language SDK for cloud compute platform with sub-250ms branching, state persistence, and parallel execution capabilities.

## Core Architecture

### Base Client

```typescript
interface ClientConfig {
  apiKey: string;
  endpoint?: string;
  timeout?: number;
  maxRetries?: number;
}

class Client {
  snapshots: SnapshotManager;
  instances: InstanceManager;
  images: ImageManager;
  branches: BranchManager;
}
```

### Authentication

- API key-based authentication
- Environment variable support: `FAAS_API_KEY`
- Session token refresh mechanism

## API Modules

### 1. Snapshot Management

```typescript
interface Snapshot {
  id: string;
  parentId?: string;
  metadata: SnapshotMetadata;
  createdAt: Date;
  size: number;
  checksum: string;
}

interface SnapshotOperations {
  create(config: SnapshotConfig): Promise<Snapshot>;
  get(id: string): Promise<Snapshot>;
  list(filter?: SnapshotFilter): Promise<Snapshot[]>;
  delete(id: string): Promise<void>;
  clone(id: string): Promise<Snapshot>;
  diff(baseId: string, targetId: string): Promise<SnapshotDiff>;
  merge(ids: string[]): Promise<Snapshot>;
}

interface SnapshotConfig {
  baseImage?: string;
  vcpus?: number;
  memory?: number;
  diskSize?: number;
  environment?: Record<string, string>;
  networkConfig?: NetworkConfig;
}
```

### 2. Instance Management

```typescript
interface Instance {
  id: string;
  snapshotId: string;
  state: InstanceState;
  endpoints: InstanceEndpoints;
  resources: ResourceConfig;
}

interface InstanceOperations {
  start(config: InstanceConfig): Promise<Instance>;
  stop(id: string): Promise<void>;
  pause(id: string): Promise<void>;
  resume(id: string): Promise<void>;
  restart(id: string): Promise<void>;
  list(filter?: InstanceFilter): Promise<Instance[]>;
  get(id: string): Promise<Instance>;
  waitUntilReady(id: string, timeout?: number): Promise<void>;
  getMetrics(id: string): Promise<InstanceMetrics>;
}

interface InstanceConfig {
  snapshotId: string;
  autoStop?: boolean;
  ttl?: number;
  resources?: ResourceConfig;
}
```

### 3. Execution Operations

```typescript
interface ExecutionResult {
  exitCode: number;
  stdout: string;
  stderr: string;
  duration: number;
  snapshotId?: string;
}

interface ExecutionOperations {
  exec(target: string | Instance, command: string): Promise<ExecutionResult>;
  execBatch(target: string | Instance, commands: string[]): Promise<ExecutionResult[]>;
  execScript(target: string | Instance, script: string): Promise<ExecutionResult>;
  execWithSnapshot(target: string | Instance, command: string): Promise<Snapshot>;
}
```

### 4. File Operations

```typescript
interface FileOperations {
  upload(target: string | Instance, localPath: string, remotePath: string): Promise<void>;
  download(target: string | Instance, remotePath: string, localPath: string): Promise<void>;
  copy(source: string | Instance, destination: string | Instance, path: string): Promise<void>;
  list(target: string | Instance, path: string): Promise<FileInfo[]>;
  mkdir(target: string | Instance, path: string): Promise<void>;
  remove(target: string | Instance, path: string): Promise<void>;
}
```

### 5. Branching Operations

```typescript
interface Branch {
  id: string;
  parentId: string;
  name?: string;
  metadata: BranchMetadata;
  divergencePoint: Snapshot;
}

interface BranchOperations {
  create(snapshotId: string, config?: BranchConfig): Promise<Branch>;
  list(parentId?: string): Promise<Branch[]>;
  merge(branchIds: string[]): Promise<Snapshot>;
  rebase(branchId: string, targetId: string): Promise<Branch>;
  diff(branchId: string, targetId: string): Promise<BranchDiff>;
  checkout(branchId: string): Promise<Instance>;
}
```

### 6. Network Operations

```typescript
interface NetworkOperations {
  exposeHttp(instance: Instance, name: string, port: number): Promise<ServiceEndpoint>;
  hideHttp(instance: Instance, name: string): Promise<void>;
  portForward(instance: Instance, localPort: number, remotePort: number): Promise<PortForward>;
  createTunnel(instance: Instance, config: TunnelConfig): Promise<Tunnel>;
  listServices(instance: Instance): Promise<ServiceEndpoint[]>;
}
```

### 7. SSH Operations

```typescript
interface SSHClient {
  connect(instance: Instance): Promise<void>;
  execCommand(command: string): Promise<ExecutionResult>;
  putFile(localPath: string, remotePath: string): Promise<void>;
  getFile(remotePath: string, localPath: string): Promise<void>;
  putDirectory(localPath: string, remotePath: string): Promise<void>;
  getDirectory(remotePath: string, localPath: string): Promise<void>;
  createShell(): Promise<InteractiveShell>;
  disconnect(): Promise<void>;
}
```

## Language-Specific Implementations

### Python SDK

```python
# sdk/python/faas_sdk/__init__.py

class FaaSClient:
    def __init__(self, api_key=None, **kwargs):
        self.snapshots = SnapshotManager(self)
        self.instances = InstanceManager(self)
        self.branches = BranchManager(self)

    def exec(self, command, target=None):
        """Execute command with automatic snapshot creation"""
        pass

    def branch(self, snapshot_id=None):
        """Create execution branch for parallel operations"""
        pass

# Context manager support
class ManagedInstance:
    def __enter__(self):
        return self.instance

    def __exit__(self, *args):
        self.instance.stop()

# Chaining operations
snapshot = client.snapshots.create()
  .exec("apt-get update")
  .exec("pip install -r requirements.txt")
  .upload("./data", "/data")
  .exec("python train.py")
```

### TypeScript SDK

```typescript
// sdk/typescript/src/index.ts

export class FaaSClient {
  constructor(config?: ClientConfig) {
    this.snapshots = new SnapshotManager(this);
    this.instances = new InstanceManager(this);
    this.branches = new BranchManager(this);
  }

  async exec(command: string, target?: string | Instance): Promise<ExecutionResult> {
    // Execute with automatic snapshot management
  }

  async branch(snapshotId?: string): Promise<Branch> {
    // Create execution branch
  }
}

// Promise chaining
const result = await client.snapshots
  .create()
  .then(s => s.exec("npm install"))
  .then(s => s.upload("./src", "/app"))
  .then(s => s.exec("npm run build"))
  .then(s => client.instances.start({ snapshotId: s.id }));

// Async iterators
for await (const instance of client.instances.list()) {
  console.log(instance.id, instance.state);
}
```

## Advanced Features

### Parallel Execution

```typescript
interface ParallelOperations {
  map<T>(items: T[], fn: (item: T, instance: Instance) => Promise<any>): Promise<any[]>;
  race(branches: Branch[]): Promise<ExecutionResult>;
  all(branches: Branch[]): Promise<ExecutionResult[]>;
  pool(size: number, tasks: Task[]): Promise<ExecutionResult[]>;
}
```

### State Persistence

```typescript
interface StateOperations {
  checkpoint(instance: Instance, name?: string): Promise<Snapshot>;
  restore(snapshotId: string): Promise<Instance>;
  timeline(instanceId: string): Promise<Snapshot[]>;
  rollback(instanceId: string, steps: number): Promise<Instance>;
}
```

### Development Environments

```typescript
interface DevEnvironment {
  type: "vscode" | "jupyter" | "desktop" | "browser";
  config: EnvironmentConfig;

  start(): Promise<Instance>;
  getAccessUrl(): string;
  installExtensions(extensions: string[]): Promise<void>;
  syncFiles(localPath: string): Promise<void>;
}
```

### AI Agent Integration

```typescript
interface AgentOperations {
  createWorkspace(config: AgentConfig): Promise<AgentWorkspace>;
  executeTask(task: AgentTask): Promise<TaskResult>;
  exploreBranches(goal: string, maxBranches: number): Promise<ExplorationResult>;
  saveCheckpoint(name: string): Promise<Snapshot>;
  revertToCheckpoint(name: string): Promise<void>;
}
```

## Error Handling

```typescript
class FaaSError extends Error {
  code: string;
  statusCode?: number;
  details?: any;
}

class SnapshotNotFoundError extends FaaSError {}
class InstanceNotReadyError extends FaaSError {}
class ResourceLimitError extends FaaSError {}
class NetworkError extends FaaSError {}
```

## Events and Callbacks

```typescript
interface EventEmitter {
  on(event: "instance.ready" | "snapshot.created" | "execution.complete", handler: Function): void;
  once(event: string, handler: Function): void;
  off(event: string, handler: Function): void;
}
```

## Resource Management

```typescript
interface ResourceConfig {
  vcpus: number;
  memory: number;  // MB
  diskSize: number; // MB
  gpus?: GPUConfig[];
  networkBandwidth?: number;
}

interface AutoScaling {
  minInstances: number;
  maxInstances: number;
  targetCPU?: number;
  targetMemory?: number;
  scaleUpThreshold?: number;
  scaleDownThreshold?: number;
}
```

## Caching and Optimization

```typescript
interface CacheConfig {
  enableSnapshotCache: boolean;
  cacheSize: number;
  ttl: number;
  compressionLevel: number;
  deduplication: boolean;
}

interface OptimizationHints {
  reuseSnapshots: boolean;
  parallelExecution: boolean;
  lazyLoading: boolean;
  incrementalSnapshots: boolean;
}
```