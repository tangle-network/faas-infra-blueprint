/**
 * FaaS Platform TypeScript SDK
 * High-level API with convenience wrappers
 */

export * from './api';
export { default as FaaSClient } from './api';

import FaaSClient, {
  Snapshot,
  Instance,
  InstanceState,
  ExecutionResult,
  Branch,
  ClientOptions,
} from './api';

// High-level Snapshot wrapper
export class ManagedSnapshot {
  constructor(
    private client: FaaSClient,
    public snapshot: Snapshot
  ) {}

  async exec(command: string): Promise<ManagedSnapshot> {
    const { snapshot, result } = await this.client.execOnSnapshot(this.snapshot.id, command);
    return new ManagedSnapshot(this.client, snapshot);
  }

  async upload(localPath: string, remotePath: string): Promise<ManagedSnapshot> {
    await this.client.uploadFile(this.snapshot.id, 'snapshot', localPath, remotePath);
    return this.exec(`echo "Uploaded ${remotePath}"`);
  }

  async download(remotePath: string, localPath: string): Promise<void> {
    await this.client.downloadFile(this.snapshot.id, 'snapshot', remotePath, localPath);
  }

  async branch(name?: string): Promise<ManagedBranch> {
    const branch = await this.client.createBranch({
      snapshot_id: this.snapshot.id,
      name,
    });
    return new ManagedBranch(this.client, branch);
  }

  async startInstance(options?: {
    ttl?: number;
    auto_stop?: boolean;
  }): Promise<ManagedInstance> {
    const instance = await this.client.startInstance({
      snapshot_id: this.snapshot.id,
      ...options,
    });
    await this.client.waitForInstance(instance.id, InstanceState.RUNNING);
    return new ManagedInstance(this.client, instance);
  }
}

// High-level Instance wrapper
export class ManagedInstance {
  constructor(
    private client: FaaSClient,
    public instance: Instance
  ) {}

  async exec(command: string, stream = false): Promise<ExecutionResult> {
    return this.client.execOnInstance(this.instance.id, command, { stream });
  }

  async upload(localPath: string, remotePath: string): Promise<void> {
    await this.client.uploadFile(this.instance.id, 'instance', localPath, remotePath);
  }

  async download(remotePath: string, localPath: string): Promise<void> {
    await this.client.downloadFile(this.instance.id, 'instance', remotePath, localPath);
  }

  async sync(localDir: string, remoteDir: string): Promise<void> {
    await this.client.syncFiles(this.instance.id, localDir, remoteDir);
  }

  async exposeHttp(name: string, port: number): Promise<string> {
    const service = await this.client.exposeHttpService(this.instance.id, name, port);
    return service.url;
  }

  async stop(): Promise<void> {
    await this.client.stopInstance(this.instance.id);
  }

  async pause(): Promise<void> {
    await this.client.pauseInstance(this.instance.id);
  }

  async resume(): Promise<void> {
    await this.client.resumeInstance(this.instance.id);
    await this.client.waitForInstance(this.instance.id, InstanceState.RUNNING);
  }

  async delete(): Promise<void> {
    await this.client.deleteInstance(this.instance.id);
  }
}

// High-level Branch wrapper
export class ManagedBranch {
  constructor(
    private client: FaaSClient,
    public branch: Branch
  ) {}

  async exec(command: string): Promise<ExecutionResult> {
    const instance = await this.checkout();
    try {
      return await instance.exec(command);
    } finally {
      await instance.delete();
    }
  }

  async checkout(): Promise<ManagedInstance> {
    const instance = await this.client.startInstance({
      snapshot_id: this.branch.divergence_point,
    });
    await this.client.waitForInstance(instance.id, InstanceState.RUNNING);
    return new ManagedInstance(this.client, instance);
  }

  async merge(other: ManagedBranch): Promise<ManagedSnapshot> {
    const snapshot = await this.client.mergeBranches([this.branch.id, other.branch.id]);
    return new ManagedSnapshot(this.client, snapshot);
  }
}

// Convenience class with high-level API
export class FaaS extends FaaSClient {
  constructor(options?: ClientOptions) {
    super(options);
  }

  async snapshot(options?: {
    base_image?: string;
    vcpus?: number;
    memory?: number;
    disk_size?: number;
  }): Promise<ManagedSnapshot> {
    const snapshot = await this.createSnapshot({
      base_image: options?.base_image,
      resource_spec: options
        ? {
            vcpus: options.vcpus || 1,
            memory: options.memory || 512,
            disk_size: options.disk_size || 10240,
          }
        : undefined,
    });
    return new ManagedSnapshot(this, snapshot);
  }

  async instance(snapshotId: string): Promise<ManagedInstance> {
    const instance = await this.startInstance({ snapshot_id: snapshotId });
    await this.waitForInstance(instance.id, InstanceState.RUNNING);
    return new ManagedInstance(this, instance);
  }

  async branch(snapshotId: string, name?: string): Promise<ManagedBranch> {
    const branch = await this.createBranch({ snapshot_id: snapshotId, name });
    return new ManagedBranch(this, branch);
  }

  // Fluent API for chaining
  async chain(): Promise<ChainBuilder> {
    return new ChainBuilder(this);
  }
}

// Fluent chain builder
export class ChainBuilder {
  private operations: Array<(prev: any) => Promise<any>> = [];

  constructor(private client: FaaS) {}

  create(options?: {
    base_image?: string;
    vcpus?: number;
    memory?: number;
    disk_size?: number;
  }): ChainBuilder {
    this.operations.push(async () => {
      return this.client.snapshot(options);
    });
    return this;
  }

  exec(command: string): ChainBuilder {
    this.operations.push(async (prev: ManagedSnapshot) => {
      return prev.exec(command);
    });
    return this;
  }

  upload(localPath: string, remotePath: string): ChainBuilder {
    this.operations.push(async (prev: ManagedSnapshot) => {
      return prev.upload(localPath, remotePath);
    });
    return this;
  }

  async build(): Promise<ManagedSnapshot> {
    let result: any;
    for (const op of this.operations) {
      result = await op(result);
    }
    return result;
  }

  async deploy(): Promise<ManagedInstance> {
    const snapshot = await this.build();
    return snapshot.startInstance();
  }
}

// Export convenience function
export function createFaaS(options?: ClientOptions): FaaS {
  return new FaaS(options);
}