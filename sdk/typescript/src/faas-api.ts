/**
 * FaaS Platform SDK for Rust-based Cloud System
 * Integrates with our multi-mode execution platform
 */

import fetch, { Response } from 'node-fetch';
import WebSocket from 'ws';
import { EventEmitter } from 'events';

// Our FaaS Platform API configuration
const DEFAULT_BASE_URL = process.env.FAAS_PLATFORM_URL || 'http://localhost:8080/api/v1';
const DEFAULT_EXECUTOR_URL = process.env.FAAS_EXECUTOR_URL || 'http://localhost:8081';

// Platform execution modes matching our Rust implementation
export enum ExecutionMode {
  EPHEMERAL = 'ephemeral',
  CACHED = 'cached',
  CHECKPOINTED = 'checkpointed',
  BRANCHED = 'branched',
  PERSISTENT = 'persistent',
}

// Types matching our Rust structures
export interface PlatformConfig {
  apiKey?: string;
  platformUrl?: string;
  executorUrl?: string;
  timeout?: number;
  maxRetries?: number;
}

export interface ExecutionRequest {
  id: string;
  code: string;
  mode: ExecutionMode;
  env: string;
  timeout?: number;
  checkpoint?: string;
  branch_from?: string;
}

export interface ExecutionResponse {
  request_id: string;
  exit_code: number;
  stdout: Uint8Array;
  stderr: Uint8Array;
  duration: number;
  snapshot?: string;
  cached: boolean;
  memory_used?: number;
}

export interface Snapshot {
  id: string;
  content_hash: string;
  parent_id?: string;
  parent_hash?: string;
  mode: ExecutionMode;
  created_at: number;
  size: number;
  memory_pages?: number;
  checksum: string;
  metadata?: Record<string, any>;
}

export interface Branch {
  id: string;
  snapshot_id: string;
  parent_branch?: string;
  created_at: number;
  divergence_point: string;
  metadata?: Record<string, any>;
}

export interface Environment {
  name: string;
  image: string;
  capabilities?: string[];
  resource_limits?: ResourceLimits;
  pre_warmed?: number;
}

export interface ResourceLimits {
  cpu_cores?: number;
  memory_mb?: number;
  disk_mb?: number;
  network_mbps?: number;
}

export interface MemoryPage {
  address: bigint;
  data: Uint8Array;
  flags: number;
}

// Platform-specific error
export class PlatformError extends Error {
  constructor(
    message: string,
    public code?: string,
    public statusCode?: number,
    public details?: any
  ) {
    super(message);
    this.name = 'PlatformError';
  }
}

/**
 * Main FaaS Platform Client
 * Integrates with our Rust-based execution platform
 */
// Advanced file sync options
export interface SyncOptions {
  useGitignore?: boolean;
  dryRun?: boolean;
  deleteUnmatched?: boolean;
  checksumOnly?: boolean;
  preserveTimestamps?: boolean;
  excludePatterns?: string[];
  includePatterns?: string[];
}

export interface SyncResult {
  filesCopied: string[];
  filesUpdated: string[];
  filesDeleted: string[];
  filesSkipped: string[];
  bytesTransferred: number;
  durationMs: number;
  dryRun: boolean;
}

// SSH key management
export interface SshKeyPair {
  id: string;
  privateKey: string;
  publicKey: string;
  fingerprint: string;
  algorithm: string;
  createdAt: Date;
  expiresAt?: Date;
  rotatedFrom?: string;
}

// Readiness check configuration
export interface ReadinessConfig {
  checkInterval?: number;
  initialDelay?: number;
  timeout?: number;
  successThreshold?: number;
  failureThreshold?: number;
  probes?: ReadinessProbe[];
}

export interface ReadinessProbe {
  type: 'http' | 'tcp' | 'command' | 'file';
  path?: string;
  port?: number;
  command?: string[];
  expectedStatus?: number;
  timeout?: number;
}

export interface ReadinessStatus {
  ready: boolean;
  checksPerformed: number;
  consecutiveSuccesses: number;
  consecutiveFailures: number;
  lastCheck?: Date;
  message: string;
}

// Long-running execution support
export interface LongRunningConfig {
  maxDuration?: number;
  heartbeatInterval?: number;
  checkpointInterval?: number;
  autoExtend?: boolean;
  gracePeriod?: number;
}

export interface ExecutionSession {
  id: string;
  startedAt: Date;
  lastHeartbeat: Date;
  checkpoints: CheckpointInfo[];
  extendedCount: number;
  maxExtensions: number;
}

export interface CheckpointInfo {
  id: string;
  createdAt: Date;
  sizeBytes: number;
}

export class FaaSPlatformClient extends EventEmitter {
  private apiKey: string;
  private platformUrl: string;
  private executorUrl: string;
  private timeout: number;
  private maxRetries: number;
  private wsConnections: Map<string, WebSocket> = new Map();

  constructor(config: PlatformConfig = {}) {
    super();
    this.apiKey = config.apiKey || process.env.FAAS_API_KEY || '';
    this.platformUrl = config.platformUrl || DEFAULT_BASE_URL;
    this.executorUrl = config.executorUrl || DEFAULT_EXECUTOR_URL;
    this.timeout = config.timeout || 30000;
    this.maxRetries = config.maxRetries || 3;

    if (!this.apiKey) {
      throw new PlatformError('API key required. Set FAAS_API_KEY or pass apiKey option');
    }
  }

  // Core execution methods matching our Rust platform
  async execute(request: ExecutionRequest): Promise<ExecutionResponse> {
    const url = `${this.executorUrl}/execute`;

    const response = await this.request<ExecutionResponse>('POST', url, request);

    // Emit execution metrics
    this.emit('execution', {
      request_id: request.id,
      mode: request.mode,
      duration: response.duration,
      cached: response.cached,
      memory_used: response.memory_used,
    });

    return response;
  }

  async executeEphemeral(code: string, env = 'alpine:latest'): Promise<ExecutionResponse> {
    return this.execute({
      id: this.generateId('ephemeral'),
      code,
      mode: ExecutionMode.EPHEMERAL,
      env,
    });
  }

  async executeCached(code: string, env = 'alpine:latest'): Promise<ExecutionResponse> {
    return this.execute({
      id: this.generateId('cached'),
      code,
      mode: ExecutionMode.CACHED,
      env,
    });
  }

  async executeCheckpointed(
    code: string,
    env = 'alpine:latest',
    checkpoint?: string
  ): Promise<ExecutionResponse> {
    return this.execute({
      id: this.generateId('checkpoint'),
      code,
      mode: ExecutionMode.CHECKPOINTED,
      env,
      checkpoint,
    });
  }

  async executeBranched(
    code: string,
    branchFrom: string,
    env = 'alpine:latest'
  ): Promise<ExecutionResponse> {
    return this.execute({
      id: this.generateId('branch'),
      code,
      mode: ExecutionMode.BRANCHED,
      env,
      branch_from: branchFrom,
    });
  }

  // Snapshot operations with deterministic hashing
  async createSnapshot(executionId: string, metadata?: Record<string, any>): Promise<Snapshot> {
    return this.request<Snapshot>('POST', `${this.platformUrl}/snapshots`, {
      execution_id: executionId,
      metadata,
      deterministic: true,
    });
  }

  async restoreSnapshot(snapshotId: string): Promise<ExecutionResponse> {
    return this.request<ExecutionResponse>('POST', `${this.platformUrl}/snapshots/${snapshotId}/restore`);
  }

  async getSnapshot(id: string): Promise<Snapshot> {
    return this.request<Snapshot>('GET', `${this.platformUrl}/snapshots/${id}`);
  }

  async listSnapshots(filter?: {
    mode?: ExecutionMode;
    parent_id?: string;
  }): Promise<Snapshot[]> {
    const query = new URLSearchParams(filter as any).toString();
    return this.request<Snapshot[]>('GET', `${this.platformUrl}/snapshots${query ? '?' + query : ''}`);
  }

  async deleteSnapshot(id: string): Promise<void> {
    await this.request('DELETE', `${this.platformUrl}/snapshots/${id}`);
  }

  // Branch operations for parallel execution
  async createBranch(snapshotId: string, metadata?: Record<string, any>): Promise<Branch> {
    return this.request<Branch>('POST', `${this.platformUrl}/branches`, {
      snapshot_id: snapshotId,
      metadata,
    });
  }

  async listBranches(snapshotId?: string): Promise<Branch[]> {
    const query = snapshotId ? `?snapshot_id=${snapshotId}` : '';
    return this.request<Branch[]>('GET', `${this.platformUrl}/branches${query}`);
  }

  async mergeBranches(branchIds: string[]): Promise<Snapshot> {
    return this.request<Snapshot>('POST', `${this.platformUrl}/branches/merge`, {
      branch_ids: branchIds,
    });
  }

  // Memory management for Copy-on-Write
  async getMemoryPages(snapshotId: string): Promise<MemoryPage[]> {
    return this.request<MemoryPage[]>('GET', `${this.platformUrl}/memory/${snapshotId}/pages`);
  }

  async applyMemoryDelta(
    snapshotId: string,
    pages: MemoryPage[]
  ): Promise<Snapshot> {
    return this.request<Snapshot>('POST', `${this.platformUrl}/memory/${snapshotId}/delta`, {
      pages,
    });
  }

  // Environment management
  async listEnvironments(): Promise<Environment[]> {
    return this.request<Environment[]>('GET', `${this.platformUrl}/environments`);
  }

  async preWarmEnvironment(name: string, count: number): Promise<void> {
    await this.request('POST', `${this.platformUrl}/environments/${name}/warm`, {
      count,
    });
  }

  // WebSocket for streaming execution
  async streamExecution(request: ExecutionRequest): Promise<WebSocket> {
    const ws = new WebSocket(`${this.executorUrl.replace('http', 'ws')}/stream`);

    ws.on('open', () => {
      ws.send(JSON.stringify({
        ...request,
        api_key: this.apiKey,
      }));
    });

    ws.on('message', (data) => {
      const message = JSON.parse(data.toString());
      this.emit('stream', message);
    });

    this.wsConnections.set(request.id, ws);
    return ws;
  }

  // Parallel execution helpers
  async parallelMap<T, R>(
    items: T[],
    fn: (item: T, snapshot: Snapshot) => Promise<R>,
    baseSnapshot?: string
  ): Promise<R[]> {
    // Create branches for parallel execution
    const branches = await Promise.all(
      items.map(() =>
        baseSnapshot
          ? this.createBranch(baseSnapshot)
          : this.createSnapshot(this.generateId('parallel'))
      )
    );

    try {
      // Execute in parallel
      const results = await Promise.all(
        items.map((item, i) => {
          const snapshot = 'snapshot_id' in branches[i]
            ? { id: branches[i].snapshot_id } as Snapshot
            : branches[i] as Snapshot;
          return fn(item, snapshot);
        })
      );

      return results;
    } finally {
      // Cleanup branches
      await Promise.all(
        branches.map(b =>
          'snapshot_id' in b
            ? this.deleteSnapshot(b.snapshot_id)
            : this.deleteSnapshot(b.id)
        )
      );
    }
  }

  async race(executions: ExecutionRequest[]): Promise<ExecutionResponse> {
    return Promise.race(executions.map(req => this.execute(req)));
  }

  // Advanced file sync
  async syncFiles(
    instanceId: string,
    localDir: string,
    remoteDir: string,
    options?: SyncOptions
  ): Promise<SyncResult> {
    return this.request<SyncResult>('POST', `${this.platformUrl}/instances/${instanceId}/sync`, {
      local_dir: localDir,
      remote_dir: remoteDir,
      options: options || {},
    });
  }

  // SSH key rotation
  async rotateSSHKey(instanceId: string): Promise<SshKeyPair> {
    return this.request<SshKeyPair>('POST', `${this.platformUrl}/instances/${instanceId}/ssh/rotate`);
  }

  async getSSHKey(instanceId: string): Promise<SshKeyPair> {
    return this.request<SshKeyPair>('GET', `${this.platformUrl}/instances/${instanceId}/ssh`);
  }

  // Readiness checks
  async waitForReady(
    instanceId: string,
    config?: ReadinessConfig
  ): Promise<ReadinessStatus> {
    const startTime = Date.now();
    const timeout = config?.timeout || 60000;
    const checkInterval = config?.checkInterval || 1000;

    while (Date.now() - startTime < timeout) {
      const status = await this.checkReadiness(instanceId, config);

      if (status.ready) {
        return status;
      }

      await this.delay(checkInterval);
    }

    throw new PlatformError(`Instance ${instanceId} not ready after ${timeout}ms`, 'TIMEOUT');
  }

  async checkReadiness(
    instanceId: string,
    config?: ReadinessConfig
  ): Promise<ReadinessStatus> {
    return this.request<ReadinessStatus>('POST', `${this.platformUrl}/instances/${instanceId}/readiness`, {
      config,
    });
  }

  // Long-running execution support
  async startLongRunningExecution(
    request: ExecutionRequest,
    config?: LongRunningConfig
  ): Promise<ExecutionSession> {
    const session = await this.request<ExecutionSession>('POST', `${this.platformUrl}/executions/long-running`, {
      request,
      config: config || {
        maxDuration: 24 * 60 * 60 * 1000, // 24 hours
        heartbeatInterval: 30000, // 30 seconds
        checkpointInterval: 300000, // 5 minutes
      },
    });

    // Start heartbeat
    this.startHeartbeat(session.id, config?.heartbeatInterval || 30000);

    return session;
  }

  private heartbeatIntervals: Map<string, NodeJS.Timeout> = new Map();

  private startHeartbeat(sessionId: string, interval: number): void {
    const heartbeat = setInterval(async () => {
      try {
        await this.sendHeartbeat(sessionId);
      } catch (error) {
        console.error(`Heartbeat failed for session ${sessionId}:`, error);
        this.stopHeartbeat(sessionId);
      }
    }, interval);

    this.heartbeatIntervals.set(sessionId, heartbeat);
  }

  private stopHeartbeat(sessionId: string): void {
    const interval = this.heartbeatIntervals.get(sessionId);
    if (interval) {
      clearInterval(interval);
      this.heartbeatIntervals.delete(sessionId);
    }
  }

  async sendHeartbeat(sessionId: string): Promise<void> {
    await this.request('POST', `${this.platformUrl}/executions/${sessionId}/heartbeat`);
  }

  async extendSession(sessionId: string): Promise<number> {
    const result = await this.request<{ new_duration: number }>(
      'POST',
      `${this.platformUrl}/executions/${sessionId}/extend`
    );
    return result.new_duration;
  }

  // Stream execution with callbacks (Morph-style)
  async executeWithCallbacks(
    request: ExecutionRequest,
    options: {
      onStdout?: (data: string) => void;
      onStderr?: (data: string) => void;
      onProgress?: (percent: number) => void;
      timeout?: number;
    } = {}
  ): Promise<ExecutionResponse> {
    const ws = new WebSocket(`${this.platformUrl.replace('http', 'ws')}/stream/${request.id}`);

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        ws.close();
        reject(new PlatformError('Execution timeout', 'TIMEOUT'));
      }, options.timeout || 300000);

      ws.on('open', () => {
        ws.send(JSON.stringify({ type: 'subscribe', execution_id: request.id }));
        // Send execution request
        this.execute(request).catch(reject);
      });

      ws.on('message', (data: string) => {
        const msg = JSON.parse(data);
        switch (msg.type) {
          case 'output':
            if (msg.stream === 'stdout' && options.onStdout) {
              options.onStdout(msg.data);
            } else if (msg.stream === 'stderr' && options.onStderr) {
              options.onStderr(msg.data);
            }
            break;
          case 'progress':
            if (options.onProgress) {
              options.onProgress(msg.progress);
            }
            break;
          case 'completed':
            clearTimeout(timeout);
            ws.close();
            resolve({
              request_id: request.id,
              exit_code: msg.exit_code,
              stdout: new Uint8Array(),
              stderr: new Uint8Array(),
              duration: 0,
              cached: false,
            });
            break;
          case 'error':
            clearTimeout(timeout);
            ws.close();
            reject(new PlatformError(msg.error, 'EXECUTION_ERROR'));
            break;
        }
      });

      ws.on('error', (error) => {
        clearTimeout(timeout);
        reject(error);
      });
    });
  }

  // Performance metrics
  async getMetrics(): Promise<{
    cache_hit_rate: number;
    avg_cold_start_ms: number;
    avg_warm_start_ms: number;
    total_executions: number;
    active_snapshots: number;
  }> {
    return this.request('GET', `${this.platformUrl}/metrics`);
  }

  // Helper methods
  private async request<T>(
    method: string,
    url: string,
    body?: any,
    options: any = {}
  ): Promise<T> {
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
          throw new PlatformError(
            `HTTP ${response.status}`,
            undefined,
            response.status,
            await response.text()
          );
        }

        return await response.json() as T;
      } catch (error: any) {
        attempt++;

        if (error.name === 'AbortError') {
          throw new PlatformError(`Request timeout after ${this.timeout}ms`, 'TIMEOUT');
        }

        if (attempt >= this.maxRetries || (error.statusCode && error.statusCode < 500)) {
          throw error;
        }

        await this.delay(Math.pow(2, attempt) * 1000);
      }
    }

    throw new PlatformError(`Max retries (${this.maxRetries}) exceeded`);
  }

  private generateId(prefix: string): string {
    return `${prefix}-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  // Cleanup
  async close(): Promise<void> {
    for (const [id, ws] of this.wsConnections) {
      ws.close();
      this.wsConnections.delete(id);
    }
  }
}

// High-level fluent API
export class FluentExecutor {
  private client: FaaSPlatformClient;
  private currentSnapshot?: string;
  private operations: Array<() => Promise<any>> = [];

  constructor(client: FaaSPlatformClient) {
    this.client = client;
  }

  from(env: string): FluentExecutor {
    this.operations.push(async () => {
      const result = await this.client.executeCached('', env);
      this.currentSnapshot = result.snapshot;
    });
    return this;
  }

  exec(code: string): FluentExecutor {
    this.operations.push(async () => {
      const result = await this.client.executeCheckpointed(
        code,
        'alpine:latest',
        this.currentSnapshot
      );
      this.currentSnapshot = result.snapshot;
    });
    return this;
  }

  branch(): FluentExecutor {
    this.operations.push(async () => {
      if (!this.currentSnapshot) {
        throw new PlatformError('No snapshot to branch from');
      }
      const branch = await this.client.createBranch(this.currentSnapshot);
      this.currentSnapshot = branch.snapshot_id;
    });
    return this;
  }

  async run(): Promise<ExecutionResponse | undefined> {
    let lastResult: ExecutionResponse | undefined;

    for (const op of this.operations) {
      lastResult = await op();
    }

    return lastResult;
  }

  async snapshot(): Promise<string> {
    await this.run();
    if (!this.currentSnapshot) {
      throw new PlatformError('No snapshot created');
    }
    return this.currentSnapshot;
  }
}

// Export convenience functions
export function createClient(config?: PlatformConfig): FaaSPlatformClient {
  return new FaaSPlatformClient(config);
}

export function fluent(client: FaaSPlatformClient): FluentExecutor {
  return new FluentExecutor(client);
}