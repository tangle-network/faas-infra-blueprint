import axios, { AxiosInstance } from 'axios';
import WebSocket from 'ws';



export interface SnapshotSpec {
  image?: string;
  containerID?: string;
  name: string;
  vcpus?: number;
  memory?: number; // MB
  diskSize?: number; // MB
}

export interface Snapshot {
  id: string;
  name: string;
  createdAt: number;
  sizeBytes: number;
  image?: string;
}

export interface Instance {
  id: string;
  status: 'starting' | 'running' | 'paused' | 'stopped';
  ssh?: SSHInfo;
  createdAt: number;
  resources: {
    cpuCores: number;
    memoryMb: number;
    diskGb: number;
  };
}

export interface SSHInfo {
  host: string;
  port: number;
  username: string;
  privateKey?: string;
}

export interface ExecResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

export interface Branch {
  id: string;
  parentSnapshotId: string;
  name: string;
  createdAt: number;
}


export class FaaSClient {
  private api: AxiosInstance;
  public snapshots: SnapshotManager;
  public instances: InstanceManager;
  public branches: BranchManager;

  constructor(options: {
    apiKey?: string;
    endpoint?: string;
  } = {}) {
    this.api = axios.create({
      baseURL: options.endpoint || process.env.FAAS_API_URL || 'http://localhost:8080',
      headers: {
        'x-api-key': options.apiKey || process.env.FAAS_API_KEY || 'dev-api-key',
      },
    });

    this.snapshots = new SnapshotManager(this.api);
    this.instances = new InstanceManager(this.api);
    this.branches = new BranchManager(this.api);
  }

  async execute(command: string, image = 'alpine:latest'): Promise<ExecResult> {
    const response = await this.api.post('/api/v1/execute', {
      image,
      command: command.split(' '),
      env_vars: null,
      payload: [],
    });

    return {
      stdout: response.data.response ? Buffer.from(response.data.response).toString() : '',
      stderr: response.data.logs || '',
      exitCode: response.data.error ? 1 : 0,
    };
  }

  async executeAdvanced(options: {
    command: string;
    image?: string;
    mode?: 'ephemeral' | 'cached' | 'checkpointed' | 'branched' | 'persistent';
    checkpointId?: string;
    branchFrom?: string;
    timeout?: number;
  }): Promise<ExecResult> {
    const response = await this.api.post('/api/v1/execute/advanced', {
      image: options.image || 'alpine:latest',
      command: options.command.split(' '),
      env_vars: null,
      payload: [],
      mode: options.mode || 'ephemeral',
      checkpoint_id: options.checkpointId,
      branch_from: options.branchFrom,
      timeout_secs: options.timeout,
    });

    return {
      stdout: response.data.response ? Buffer.from(response.data.response).toString() : '',
      stderr: response.data.logs || '',
      exitCode: response.data.error ? 1 : 0,
    };
  }
}


class SnapshotManager {
  constructor(private api: AxiosInstance) {}

  async create(spec: SnapshotSpec): Promise<Snapshot> {
    const response = await this.api.post('/api/v1/snapshots', {
      container_id: spec.containerID || `new_${Date.now()}`,
      name: spec.name,
      description: `vcpus:${spec.vcpus || 1} memory:${spec.memory || 1024}`,
    });

    return {
      id: response.data.snapshot_id,
      name: spec.name,
      createdAt: Date.now(),
      sizeBytes: 0,
      image: spec.image,
    };
  }

  async list(): Promise<Snapshot[]> {
    const response = await this.api.get('/api/v1/snapshots');
    return response.data;
  }

  async restore(snapshotId: string): Promise<string> {
    const response = await this.api.post(`/api/v1/snapshots/${snapshotId}/restore`);
    return response.data.container_id;
  }
}

class InstanceManager {
  constructor(private api: AxiosInstance) {}

  async start(options: {
    snapshotId?: string;
    image?: string;
    cpuCores?: number;
    memoryMb?: number;
    diskGb?: number;
    enableSSH?: boolean;
  }): Promise<InstanceProxy> {
    const response = await this.api.post('/api/v1/instances', {
      snapshot_id: options.snapshotId,
      image: options.image || 'alpine:latest',
      cpu_cores: options.cpuCores || 1,
      memory_mb: options.memoryMb || 1024,
      disk_gb: options.diskGb || 10,
      enable_ssh: options.enableSSH || false,
    });

    const instance: Instance = {
      id: response.data.instance_id,
      status: 'starting',
      createdAt: Date.now(),
      resources: {
        cpuCores: options.cpuCores || 1,
        memoryMb: options.memoryMb || 1024,
        diskGb: options.diskGb || 10,
      },
    };

    return new InstanceProxy(instance, this.api);
  }

  async get(instanceId: string): Promise<Instance> {
    const response = await this.api.get(`/api/v1/instances/${instanceId}/info`);
    return response.data;
  }

  async list(): Promise<Instance[]> {
    const response = await this.api.get('/api/v1/instances');
    return response.data;
  }
}

class BranchManager {
  constructor(private api: AxiosInstance) {}

  async create(parentSnapshotId: string, name: string): Promise<Branch> {
    const response = await this.api.post('/api/v1/branches', {
      parent_snapshot_id: parentSnapshotId,
      branch_name: name,
    });

    return {
      id: response.data.branch_id,
      parentSnapshotId,
      name,
      createdAt: Date.now(),
    };
  }

  async merge(branchIds: string[], strategy: 'union' | 'intersection' | 'latest' = 'latest'): Promise<string> {
    const response = await this.api.post('/api/v1/branches/merge', {
      branch_ids: branchIds,
      merge_strategy: strategy,
    });

    return response.data.merged_id;
  }
}


export class InstanceProxy {
  constructor(
    private instance: Instance,
    private api: AxiosInstance,
  ) {}

  get id(): string {
    return this.instance.id;
  }

  get status(): string {
    return this.instance.status;
  }

  async exec(command: string): Promise<ExecResult> {
    const response = await this.api.post('/api/v1/execute/advanced', {
      image: 'use-instance',
      command: command.split(' '),
      env_vars: null,
      payload: [],
      mode: 'persistent',
      checkpoint_id: this.instance.id,
    });

    return {
      stdout: response.data.response ? Buffer.from(response.data.response).toString() : '',
      stderr: response.data.logs || '',
      exitCode: response.data.error ? 1 : 0,
    };
  }

  async snapshot(name?: string): Promise<Snapshot> {
    const response = await this.api.post('/api/v1/snapshots', {
      container_id: this.instance.id,
      name: name || `snapshot-${this.instance.id}-${Date.now()}`,
      description: 'Instance snapshot',
    });

    return {
      id: response.data.snapshot_id,
      name: name || `snapshot-${this.instance.id}`,
      createdAt: Date.now(),
      sizeBytes: 0,
    };
  }

  async stop(): Promise<void> {
    await this.api.post(`/api/v1/instances/${this.instance.id}/stop`);
    this.instance.status = 'stopped';
  }

  async pause(): Promise<string> {
    const response = await this.api.post(`/api/v1/instances/${this.instance.id}/pause`);
    this.instance.status = 'paused';
    return response.data.checkpoint_id;
  }

  async exposePort(port: number, protocol: 'http' | 'https' | 'tcp' = 'http', subdomain?: string): Promise<string> {
    const response = await this.api.post('/api/v1/ports/expose', {
      instance_id: this.instance.id,
      internal_port: port,
      protocol,
      subdomain,
    });

    return response.data.public_url;
  }

  async uploadFiles(targetPath: string, files: Buffer): Promise<void> {
    await this.api.post('/api/v1/files/upload', {
      instance_id: this.instance.id,
      target_path: targetPath,
      files_data: Array.from(files),
    });
  }

  async getSSH(): Promise<SSHInfo> {
    const response = await this.api.get(`/api/v1/instances/${this.instance.id}/info`);
    return {
      host: response.data.ssh_host || 'localhost',
      port: response.data.ssh_port || 22,
      username: response.data.ssh_username || 'faas',
    };
  }
}


