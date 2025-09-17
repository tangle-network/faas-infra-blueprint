/**
 * Unified FaaS Client
 * Seamlessly switches between Direct HTTP and Tangle blockchain flows
 */

import { FaaSPlatformClient, ExecutionMode, ExecutionRequest, ExecutionResponse, Snapshot } from './faas-api';
import { TangleFaaSClient, TangleJobExecutor, BlueprintJobArgs } from './tangle-integration';
import { Keyring } from '@polkadot/api';
import { KeyringPair } from '@polkadot/keyring/types';
import IPFS from 'ipfs-core';

export interface UnifiedConfig {
  mode: 'direct' | 'tangle' | 'auto';

  // Direct API config
  apiKey?: string;
  platformUrl?: string;
  executorUrl?: string;

  // Tangle config
  tangleEndpoint?: string;
  blueprintId?: number;
  keypair?: KeyringPair | string;

  // Auto-mode preferences
  preferTangle?: boolean;
  costThreshold?: bigint; // Use Tangle if cost < threshold
  latencyThreshold?: number; // Use Direct if latency critical
}

export interface UnifiedExecutionOptions {
  mode?: ExecutionMode;
  env?: string;
  checkpoint?: string;
  branchFrom?: string;

  // Execution preferences
  verifiable?: boolean; // Force Tangle for verifiable execution
  decentralized?: boolean; // Force Tangle for decentralization
  lowLatency?: boolean; // Force Direct for low latency

  // Resource requirements
  cpuCores?: number;
  memoryMb?: number;
  timeoutMs?: number;

  // File operations
  uploadFiles?: Array<{ local: string; remote: string }>;
  downloadFiles?: Array<{ remote: string; local: string }>;
}

export class UnifiedFaaSClient {
  private directClient?: FaaSPlatformClient;
  private tangleClient?: TangleFaaSClient;
  private tangleExecutor?: TangleJobExecutor;
  private ipfs?: IPFS.IPFS;
  private config: UnifiedConfig;
  private keypair?: KeyringPair;

  constructor(config: UnifiedConfig) {
    this.config = config;

    // Initialize Direct client if configured
    if (config.mode === 'direct' || config.mode === 'auto') {
      if (config.apiKey) {
        this.directClient = new FaaSPlatformClient({
          apiKey: config.apiKey,
          platformUrl: config.platformUrl,
          executorUrl: config.executorUrl,
        });
      }
    }

    // Initialize Tangle client if configured
    if (config.mode === 'tangle' || config.mode === 'auto') {
      this.tangleClient = new TangleFaaSClient({
        endpoint: config.tangleEndpoint,
        blueprintId: config.blueprintId,
      });

      if (config.keypair) {
        this.keypair = typeof config.keypair === 'string'
          ? new Keyring({ type: 'sr25519' }).addFromUri(config.keypair)
          : config.keypair;

        this.tangleExecutor = new TangleJobExecutor(this.tangleClient, this.keypair);
      }
    }

    // Validate configuration
    if (!this.directClient && !this.tangleClient) {
      throw new Error('No execution backend configured');
    }
  }

  async connect(): Promise<void> {
    // Connect to Tangle if configured
    if (this.tangleClient) {
      await this.tangleClient.connect();
    }

    // Initialize IPFS for file operations
    if (this.config.mode === 'tangle' || this.config.mode === 'auto') {
      this.ipfs = await IPFS.create();
    }
  }

  async disconnect(): Promise<void> {
    if (this.tangleClient) {
      await this.tangleClient.disconnect();
    }
    if (this.ipfs) {
      await this.ipfs.stop();
    }
    if (this.directClient) {
      await this.directClient.close();
    }
  }

  /**
   * Execute code with automatic flow selection
   */
  async execute(
    code: string,
    options: UnifiedExecutionOptions = {}
  ): Promise<ExecutionResponse> {
    // Handle file uploads if specified
    if (options.uploadFiles && options.uploadFiles.length > 0) {
      code = await this.wrapWithFileUploads(code, options.uploadFiles);
    }

    // Determine which flow to use
    const useTangle = this.shouldUseTangle(options);

    let result: ExecutionResponse;

    if (useTangle && this.tangleClient && this.keypair) {
      result = await this.executeTangle(code, options);
    } else if (this.directClient) {
      result = await this.executeDirect(code, options);
    } else {
      throw new Error('No suitable execution backend available');
    }

    // Handle file downloads if specified
    if (options.downloadFiles && options.downloadFiles.length > 0) {
      await this.handleFileDownloads(result, options.downloadFiles);
    }

    return result;
  }

  /**
   * Execute using Direct HTTP API
   */
  private async executeDirect(
    code: string,
    options: UnifiedExecutionOptions
  ): Promise<ExecutionResponse> {
    if (!this.directClient) {
      throw new Error('Direct client not configured');
    }

    const request: ExecutionRequest = {
      id: this.generateId('direct'),
      code,
      mode: options.mode || ExecutionMode.EPHEMERAL,
      env: options.env || 'alpine:latest',
      timeout: options.timeoutMs,
      checkpoint: options.checkpoint,
      branch_from: options.branchFrom,
    };

    return this.directClient.execute(request);
  }

  /**
   * Execute using Tangle blockchain
   */
  private async executeTangle(
    code: string,
    options: UnifiedExecutionOptions
  ): Promise<ExecutionResponse> {
    if (!this.tangleClient || !this.keypair) {
      throw new Error('Tangle client not configured');
    }

    const jobArgs: BlueprintJobArgs = {
      execution_mode: this.mapExecutionMode(options.mode || ExecutionMode.EPHEMERAL),
      code,
      environment: options.env || 'alpine:latest',
      checkpoint_id: options.checkpoint,
      branch_from: options.branchFrom,
      resource_requirements: options.cpuCores || options.memoryMb || options.timeoutMs
        ? {
            cpu_cores: options.cpuCores || 1,
            memory_mb: options.memoryMb || 512,
            timeout_ms: options.timeoutMs || 30000,
          }
        : undefined,
    };

    const submission = await this.tangleClient.submitJob(jobArgs, this.keypair);
    const result = await this.tangleClient.waitForJobResult(
      submission.job_id,
      options.timeoutMs
    );

    // Convert Tangle result to ExecutionResponse
    return {
      request_id: `tangle-job-${result.job_id}`,
      exit_code: result.exit_code,
      stdout: Buffer.from(result.stdout),
      stderr: Buffer.from(result.stderr),
      duration: result.execution_time_ms / 1000,
      snapshot: result.snapshot_id,
      cached: false,
      memory_used: undefined,
    };
  }

  /**
   * Determine whether to use Tangle or Direct API
   */
  private shouldUseTangle(options: UnifiedExecutionOptions): boolean {
    // Explicit mode preferences
    if (options.verifiable || options.decentralized) {
      return true;
    }
    if (options.lowLatency) {
      return false;
    }

    // Auto mode decision logic
    if (this.config.mode === 'auto') {
      // Use Tangle if Direct not available
      if (!this.directClient) {
        return true;
      }

      // Use Direct if Tangle not available
      if (!this.tangleClient || !this.keypair) {
        return false;
      }

      // Use config preferences
      if (this.config.preferTangle !== undefined) {
        return this.config.preferTangle;
      }

      // Default to Direct for lower latency
      return false;
    }

    // Use configured mode
    return this.config.mode === 'tangle';
  }

  /**
   * Create snapshot with automatic flow selection
   */
  async createSnapshot(executionId: string): Promise<Snapshot> {
    if (this.directClient) {
      return this.directClient.createSnapshot(executionId);
    } else if (this.tangleClient) {
      // For Tangle, snapshots are created as part of job execution
      throw new Error('Snapshot creation via Tangle requires job execution');
    } else {
      throw new Error('No execution backend available');
    }
  }

  /**
   * Parallel execution with automatic flow selection
   */
  async parallelMap<T, R>(
    items: T[],
    fn: (item: T) => string,
    options: UnifiedExecutionOptions = {}
  ): Promise<R[]> {
    const useTangle = this.shouldUseTangle(options);

    if (useTangle && this.tangleExecutor) {
      const results = await this.tangleExecutor.parallelMap(
        items,
        fn,
        options.env
      );
      return results as any as R[];
    } else if (this.directClient) {
      // Use direct client's parallel execution
      const executions = items.map(item => ({
        id: this.generateId('parallel'),
        code: fn(item),
        mode: options.mode || ExecutionMode.CACHED,
        env: options.env || 'alpine:latest',
      }));

      const results = await Promise.all(
        executions.map(req => this.directClient!.execute(req))
      );

      return results as any as R[];
    } else {
      throw new Error('No execution backend available');
    }
  }

  /**
   * Fluent execution builder
   */
  chain(): ChainBuilder {
    return new ChainBuilder(this);
  }

  // File operations helpers

  private async wrapWithFileUploads(
    code: string,
    files: Array<{ local: string; remote: string }>
  ): Promise<string> {
    if (this.shouldUseTangle({ decentralized: true }) && this.ipfs) {
      // Upload files to IPFS for Tangle execution
      const uploads = await Promise.all(
        files.map(async file => {
          const fs = require('fs');
          const content = fs.readFileSync(file.local);
          const result = await this.ipfs!.add(content);
          return {
            cid: result.cid.toString(),
            remote: file.remote,
          };
        })
      );

      // Prepend download commands to code
      const downloadCommands = uploads
        .map(u => `ipfs get ${u.cid} -o ${u.remote}`)
        .join(' && ');

      return `${downloadCommands} && ${code}`;
    } else {
      // For Direct API, files are uploaded separately
      // This would be handled by the platform
      return code;
    }
  }

  private async handleFileDownloads(
    result: ExecutionResponse,
    files: Array<{ remote: string; local: string }>
  ): Promise<void> {
    // Implementation would depend on how files are returned
    // For Tangle: files would be uploaded to IPFS and CIDs returned
    // For Direct: files would be downloaded via HTTP API
  }

  // Helper methods

  private generateId(prefix: string): string {
    return `${prefix}-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }

  private mapExecutionMode(mode: ExecutionMode): BlueprintJobArgs['execution_mode'] {
    const modeMap = {
      [ExecutionMode.EPHEMERAL]: 'Ephemeral',
      [ExecutionMode.CACHED]: 'Cached',
      [ExecutionMode.CHECKPOINTED]: 'Checkpointed',
      [ExecutionMode.BRANCHED]: 'Branched',
      [ExecutionMode.PERSISTENT]: 'Persistent',
    };
    return modeMap[mode] as BlueprintJobArgs['execution_mode'];
  }
}

/**
 * Fluent chain builder for unified client
 */
export class ChainBuilder {
  private client: UnifiedFaaSClient;
  private operations: Array<() => Promise<any>> = [];
  private currentSnapshot?: string;
  private options: UnifiedExecutionOptions = {};

  constructor(client: UnifiedFaaSClient) {
    this.client = client;
  }

  from(env: string): ChainBuilder {
    this.options.env = env;
    return this;
  }

  withMode(mode: ExecutionMode): ChainBuilder {
    this.options.mode = mode;
    return this;
  }

  preferTangle(): ChainBuilder {
    this.options.verifiable = true;
    return this;
  }

  preferDirect(): ChainBuilder {
    this.options.lowLatency = true;
    return this;
  }

  exec(code: string): ChainBuilder {
    this.operations.push(async () => {
      const result = await this.client.execute(code, {
        ...this.options,
        checkpoint: this.currentSnapshot,
      });
      this.currentSnapshot = result.snapshot;
      return result;
    });
    return this;
  }

  upload(local: string, remote: string): ChainBuilder {
    if (!this.options.uploadFiles) {
      this.options.uploadFiles = [];
    }
    this.options.uploadFiles.push({ local, remote });
    return this;
  }

  download(remote: string, local: string): ChainBuilder {
    if (!this.options.downloadFiles) {
      this.options.downloadFiles = [];
    }
    this.options.downloadFiles.push({ remote, local });
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
    const result = await this.run();
    return result?.snapshot || '';
  }
}

// Convenience factory function
export function createUnifiedClient(config: UnifiedConfig): UnifiedFaaSClient {
  return new UnifiedFaaSClient(config);
}