/**
 * FaaS Platform API Client
 * Complete SDK implementation with 1:1 feature parity
 */

import fetch, { Response } from 'node-fetch';
import { EventEmitter } from 'events';

// Configuration
const DEFAULT_BASE_URL = process.env.FAAS_API_URL || 'http://localhost:8080/api';
const DEFAULT_TIMEOUT = 30000;

// Types and Interfaces
export interface ClientOptions {
  apiKey?: string;
  baseUrl?: string;
  timeout?: number;
  maxRetries?: number;
}

export interface ResourceSpec {
  vcpus: number;
  memory: number;
  disk_size: number;
}

export interface SnapshotMetadata {
  name?: string;
  description?: string;
  tags?: Record<string, string>;
  environment?: Record<string, string>;
}

export interface Snapshot {
  id: string;
  parent_id?: string;
  created_at: string;
  size: number;
  checksum: string;
  metadata?: SnapshotMetadata;
  resource_spec?: ResourceSpec;
}

export interface Instance {
  id: string;
  snapshot_id: string;
  state: InstanceState;
  created_at: string;
  updated_at: string;
  endpoints?: InstanceEndpoints;
  resource_spec?: ResourceSpec;
}

export interface InstanceEndpoints {
  ssh?: string;
  http?: Record<string, string>;
  ports?: Record<number, number>;
}

export enum InstanceState {
  PENDING = 'pending',
  RUNNING = 'running',
  PAUSED = 'paused',
  STOPPED = 'stopped',
  TERMINATED = 'terminated',
}

export interface ExecutionResult {
  exit_code: number;
  stdout: string;
  stderr: string;
  duration_ms: number;
  snapshot_id?: string;
}

export interface Branch {
  id: string;
  parent_id: string;
  name?: string;
  divergence_point: string;
  created_at: string;
}

export interface HttpService {
  name: string;
  port: number;
  url: string;
  exposed_at: string;
}

export interface SSHKey {
  public_key: string;
  private_key?: string;
  fingerprint: string;
}

// Error classes
export class FaaSError extends Error {
  constructor(
    message: string,
    public code?: string,
    public statusCode?: number,
    public details?: any
  ) {
    super(message);
    this.name = 'FaaSError';
  }
}

export class TimeoutError extends FaaSError {
  constructor(message: string) {
    super(message, 'TIMEOUT', 408);
    this.name = 'TimeoutError';
  }
}

// Main API Client
export class FaaSClient extends EventEmitter {
  private apiKey: string;
  private baseUrl: string;
  private timeout: number;
  private maxRetries: number;

  constructor(options: ClientOptions = {}) {
    super();
    this.apiKey = options.apiKey || process.env.FAAS_API_KEY || '';
    this.baseUrl = options.baseUrl || DEFAULT_BASE_URL;
    this.timeout = options.timeout || DEFAULT_TIMEOUT;
    this.maxRetries = options.maxRetries || 3;

    if (!this.apiKey) {
      throw new FaaSError('API key is required. Set FAAS_API_KEY or pass apiKey option');
    }
  }

  // HTTP request helpers
  private async request<T>(
    method: string,
    path: string,
    body?: any,
    options: any = {}
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const headers = {
      'Authorization': `Bearer ${this.apiKey}`,
      'Content-Type': 'application/json',
      ...options.headers,
    };

    let attempt = 0;
    while (attempt < this.maxRetries) {
      try {
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), this.timeout);

        const response = await fetch(url, {
          method,
          headers,
          body: body ? JSON.stringify(body) : undefined,
          signal: controller.signal,
          ...options,
        });

        clearTimeout(timeoutId);

        if (!response.ok) {
          const error = await this.parseError(response);
          throw error;
        }

        const data = await response.json();
        return data as T;
      } catch (error: any) {
        attempt++;

        if (error.name === 'AbortError') {
          throw new TimeoutError(`Request timeout after ${this.timeout}ms`);
        }

        if (attempt >= this.maxRetries || error.statusCode < 500) {
          throw error;
        }

        await this.delay(Math.pow(2, attempt) * 1000);
      }
    }

    throw new FaaSError(`Max retries (${this.maxRetries}) exceeded`);
  }

  private async parseError(response: Response): Promise<FaaSError> {
    try {
      const data = await response.json();
      return new FaaSError(
        data.message || `HTTP ${response.status}`,
        data.code,
        response.status,
        data.details
      );
    } catch {
      return new FaaSError(
        `HTTP ${response.status}: ${response.statusText}`,
        undefined,
        response.status
      );
    }
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  // Snapshot operations
  async createSnapshot(options: {
    base_image?: string;
    parent_id?: string;
    resource_spec?: ResourceSpec;
    metadata?: SnapshotMetadata;
  }): Promise<Snapshot> {
    return this.request<Snapshot>('POST', '/snapshots', options);
  }

  async getSnapshot(id: string): Promise<Snapshot> {
    return this.request<Snapshot>('GET', `/snapshots/${id}`);
  }

  async listSnapshots(filter?: {
    parent_id?: string;
    tags?: Record<string, string>;
  }): Promise<Snapshot[]> {
    const query = new URLSearchParams(filter as any).toString();
    return this.request<Snapshot[]>('GET', `/snapshots${query ? '?' + query : ''}`);
  }

  async deleteSnapshot(id: string): Promise<void> {
    await this.request('DELETE', `/snapshots/${id}`);
  }

  async execOnSnapshot(
    snapshotId: string,
    command: string
  ): Promise<{ snapshot: Snapshot; result: ExecutionResult }> {
    return this.request('POST', `/snapshots/${snapshotId}/exec`, { command });
  }

  // Instance operations
  async startInstance(options: {
    snapshot_id: string;
    resource_spec?: ResourceSpec;
    ttl?: number;
    auto_stop?: boolean;
  }): Promise<Instance> {
    return this.request<Instance>('POST', '/instances', options);
  }

  async getInstance(id: string): Promise<Instance> {
    return this.request<Instance>('GET', `/instances/${id}`);
  }

  async listInstances(filter?: {
    state?: InstanceState;
    snapshot_id?: string;
  }): Promise<Instance[]> {
    const query = new URLSearchParams(filter as any).toString();
    return this.request<Instance[]>('GET', `/instances${query ? '?' + query : ''}`);
  }

  async stopInstance(id: string): Promise<void> {
    await this.request('POST', `/instances/${id}/stop`);
  }

  async pauseInstance(id: string): Promise<void> {
    await this.request('POST', `/instances/${id}/pause`);
  }

  async resumeInstance(id: string): Promise<void> {
    await this.request('POST', `/instances/${id}/resume`);
  }

  async deleteInstance(id: string): Promise<void> {
    await this.request('DELETE', `/instances/${id}`);
  }

  async execOnInstance(
    instanceId: string,
    command: string,
    options?: { stream?: boolean; timeout?: number }
  ): Promise<ExecutionResult> {
    if (options?.stream) {
      return this.streamExec(instanceId, command, options.timeout);
    }
    return this.request('POST', `/instances/${instanceId}/exec`, { command, ...options });
  }

  private async streamExec(
    instanceId: string,
    command: string,
    timeout?: number
  ): Promise<ExecutionResult> {
    const response = await fetch(`${this.baseUrl}/instances/${instanceId}/exec/stream`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ command, timeout }),
    });

    if (!response.ok) {
      throw await this.parseError(response);
    }

    const reader = response.body!.getReader();
    const decoder = new TextDecoder();
    let stdout = '';
    let stderr = '';
    let exitCode = -1;

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      const chunk = decoder.decode(value, { stream: true });
      const lines = chunk.split('\n').filter(Boolean);

      for (const line of lines) {
        try {
          const data = JSON.parse(line);
          if (data.type === 'stdout') {
            stdout += data.content;
            this.emit('stdout', data.content);
          } else if (data.type === 'stderr') {
            stderr += data.content;
            this.emit('stderr', data.content);
          } else if (data.type === 'exit') {
            exitCode = data.code;
          }
        } catch {
          // Handle non-JSON lines
        }
      }
    }

    return {
      exit_code: exitCode,
      stdout,
      stderr,
      duration_ms: 0,
    };
  }

  async waitForInstance(
    instanceId: string,
    targetState: InstanceState,
    timeoutMs = 60000
  ): Promise<Instance> {
    const start = Date.now();

    while (Date.now() - start < timeoutMs) {
      const instance = await this.getInstance(instanceId);

      if (instance.state === targetState) {
        return instance;
      }

      if (instance.state === InstanceState.TERMINATED) {
        throw new FaaSError(`Instance ${instanceId} terminated unexpectedly`);
      }

      await this.delay(1000);
    }

    throw new TimeoutError(
      `Instance ${instanceId} did not reach ${targetState} within ${timeoutMs}ms`
    );
  }

  // Branch operations
  async createBranch(options: {
    snapshot_id: string;
    name?: string;
  }): Promise<Branch> {
    return this.request<Branch>('POST', '/branches', options);
  }

  async getBranch(id: string): Promise<Branch> {
    return this.request<Branch>('GET', `/branches/${id}`);
  }

  async listBranches(filter?: {
    parent_id?: string;
  }): Promise<Branch[]> {
    const query = new URLSearchParams(filter as any).toString();
    return this.request<Branch[]>('GET', `/branches${query ? '?' + query : ''}`);
  }

  async mergeBranches(branchIds: string[]): Promise<Snapshot> {
    return this.request<Snapshot>('POST', '/branches/merge', { branch_ids: branchIds });
  }

  // File operations
  async uploadFile(
    targetId: string,
    targetType: 'instance' | 'snapshot',
    localPath: string,
    remotePath: string
  ): Promise<void> {
    const formData = new FormData();
    const fs = require('fs');
    const file = fs.createReadStream(localPath);

    formData.append('file', file);
    formData.append('remote_path', remotePath);

    await this.request(
      'POST',
      `/${targetType}s/${targetId}/files/upload`,
      formData,
      {
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
        },
      }
    );
  }

  async downloadFile(
    targetId: string,
    targetType: 'instance' | 'snapshot',
    remotePath: string,
    localPath: string
  ): Promise<void> {
    const response = await fetch(
      `${this.baseUrl}/${targetType}s/${targetId}/files/download?path=${encodeURIComponent(remotePath)}`,
      {
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
        },
      }
    );

    if (!response.ok) {
      throw await this.parseError(response);
    }

    const fs = require('fs');
    const stream = fs.createWriteStream(localPath);
    response.body!.pipe(stream);

    return new Promise((resolve, reject) => {
      stream.on('finish', resolve);
      stream.on('error', reject);
    });
  }

  async syncFiles(
    instanceId: string,
    localDir: string,
    remoteDir: string,
    options?: { exclude?: string[]; bidirectional?: boolean }
  ): Promise<void> {
    return this.request('POST', `/instances/${instanceId}/files/sync`, {
      local_dir: localDir,
      remote_dir: remoteDir,
      ...options,
    });
  }

  // Network operations
  async exposeHttpService(
    instanceId: string,
    name: string,
    port: number
  ): Promise<HttpService> {
    return this.request<HttpService>('POST', `/instances/${instanceId}/services/http`, {
      name,
      port,
    });
  }

  async hideHttpService(instanceId: string, name: string): Promise<void> {
    await this.request('DELETE', `/instances/${instanceId}/services/http/${name}`);
  }

  async listHttpServices(instanceId: string): Promise<HttpService[]> {
    return this.request<HttpService[]>('GET', `/instances/${instanceId}/services/http`);
  }

  async createPortForward(
    instanceId: string,
    localPort: number,
    remotePort: number
  ): Promise<{ local_port: number; remote_port: number; active: boolean }> {
    return this.request('POST', `/instances/${instanceId}/port-forward`, {
      local_port: localPort,
      remote_port: remotePort,
    });
  }

  // SSH operations
  async getSSHKeys(instanceId: string): Promise<SSHKey> {
    return this.request<SSHKey>('GET', `/instances/${instanceId}/ssh/keys`);
  }

  async addSSHKey(instanceId: string, publicKey: string): Promise<void> {
    await this.request('POST', `/instances/${instanceId}/ssh/keys`, {
      public_key: publicKey,
    });
  }

  // Helper methods for chaining operations
  async chainOperations(operations: Array<(prev: any) => Promise<any>>): Promise<any> {
    let result = null;
    for (const op of operations) {
      result = await op(result);
    }
    return result;
  }

  // Parallel execution
  async parallelExec<T>(
    items: T[],
    fn: (item: T, instance: Instance) => Promise<any>
  ): Promise<any[]> {
    const instances = await Promise.all(
      items.map(async () => {
        const snapshot = await this.createSnapshot({});
        return this.startInstance({ snapshot_id: snapshot.id });
      })
    );

    try {
      const results = await Promise.all(
        items.map((item, i) => fn(item, instances[i]))
      );
      return results;
    } finally {
      await Promise.all(instances.map(i => this.deleteInstance(i.id)));
    }
  }
}

// Export convenience functions
export function createClient(options?: ClientOptions): FaaSClient {
  return new FaaSClient(options);
}

export default FaaSClient;