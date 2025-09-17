/**
 * Tangle Network Integration for FaaS Blueprint
 * Polkadot.js API integration for job submission and management
 */

import { ApiPromise, WsProvider, Keyring } from '@polkadot/api';
import { KeyringPair } from '@polkadot/keyring/types';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { EventRecord } from '@polkadot/types/interfaces';
import { u8aToHex, hexToU8a, stringToHex } from '@polkadot/util';
import { EventEmitter } from 'events';

// Blueprint-specific types for our FaaS system
export interface BlueprintJobArgs {
  execution_mode: 'Ephemeral' | 'Cached' | 'Checkpointed' | 'Branched' | 'Persistent';
  code: string;
  environment: string;
  resource_requirements?: {
    cpu_cores: number;
    memory_mb: number;
    timeout_ms: number;
  };
  checkpoint_id?: string;
  branch_from?: string;
}

export interface JobSubmissionResult {
  job_id: number;
  block_hash: string;
  extrinsic_hash: string;
  events: JobEvent[];
}

export interface JobEvent {
  method: string;
  section: string;
  data: any[];
}

export interface JobResult {
  job_id: number;
  exit_code: number;
  stdout: string;
  stderr: string;
  execution_time_ms: number;
  snapshot_id?: string;
  gas_used: bigint;
}

export interface TangleConfig {
  endpoint?: string;
  blueprintId?: number;
  keyringType?: 'sr25519' | 'ed25519';
  ss58Format?: number;
}

/**
 * Tangle Blueprint Client for FaaS Job Management
 */
export class TangleFaaSClient extends EventEmitter {
  private api?: ApiPromise;
  private wsProvider: WsProvider;
  private keyring: Keyring;
  private blueprintId: number;
  private jobListeners: Map<number, (result: JobResult) => void> = new Map();

  constructor(config: TangleConfig = {}) {
    super();

    const endpoint = config.endpoint || process.env.TANGLE_RPC_URL || 'ws://localhost:9944';
    this.wsProvider = new WsProvider(endpoint);
    this.blueprintId = config.blueprintId || parseInt(process.env.FAAS_BLUEPRINT_ID || '1');

    this.keyring = new Keyring({
      type: config.keyringType || 'sr25519',
      ss58Format: config.ss58Format || 42,
    });
  }

  /**
   * Initialize connection to Tangle network
   */
  async connect(): Promise<void> {
    this.api = await ApiPromise.create({
      provider: this.wsProvider,
      types: this.getCustomTypes(),
    });

    await this.api.isReady;

    // Subscribe to system events
    this.subscribeToEvents();

    this.emit('connected', {
      chain: await this.api.rpc.system.chain(),
      version: await this.api.rpc.system.version(),
    });
  }

  /**
   * Submit job to FaaS blueprint on Tangle
   */
  async submitJob(
    args: BlueprintJobArgs,
    account: KeyringPair | string,
    options?: {
      maxGas?: bigint;
      tip?: bigint;
      nonce?: number;
    }
  ): Promise<JobSubmissionResult> {
    if (!this.api) throw new Error('Not connected to Tangle');

    const signer = typeof account === 'string'
      ? this.keyring.addFromUri(account)
      : account;

    // Encode job arguments for the blueprint
    const encodedArgs = this.encodeJobArgs(args);

    // Create job submission extrinsic
    const extrinsic = this.api.tx.jobs.submitJob(
      this.blueprintId,
      encodedArgs,
      options?.maxGas || 1000000000000n
    );

    // Sign and submit
    return new Promise((resolve, reject) => {
      const events: JobEvent[] = [];
      let jobId: number | undefined;

      extrinsic.signAndSend(
        signer,
        { nonce: options?.nonce, tip: options?.tip },
        ({ status, events: rawEvents, dispatchError }) => {
          if (status.isInBlock || status.isFinalized) {
            const blockHash = status.asInBlock || status.asFinalized;

            rawEvents.forEach(({ event }) => {
              const eventData = {
                method: event.method,
                section: event.section,
                data: event.data.toJSON(),
              };
              events.push(eventData);

              // Check for job creation event
              if (event.section === 'jobs' && event.method === 'JobSubmitted') {
                jobId = event.data[1].toNumber(); // Extract job ID
                this.emit('jobSubmitted', { jobId, args });
              }

              // Check for errors
              if (event.section === 'system' && event.method === 'ExtrinsicFailed') {
                if (dispatchError) {
                  const error = this.decodeError(dispatchError);
                  reject(new Error(`Job submission failed: ${error}`));
                }
              }
            });

            if (status.isFinalized && jobId !== undefined) {
              resolve({
                job_id: jobId,
                block_hash: blockHash.toString(),
                extrinsic_hash: extrinsic.hash.toHex(),
                events,
              });
            }
          }
        }
      ).catch(reject);
    });
  }

  /**
   * Wait for job completion and get results
   */
  async waitForJobResult(
    jobId: number,
    timeoutMs = 60000
  ): Promise<JobResult> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.jobListeners.delete(jobId);
        reject(new Error(`Job ${jobId} timed out after ${timeoutMs}ms`));
      }, timeoutMs);

      this.jobListeners.set(jobId, (result) => {
        clearTimeout(timeout);
        this.jobListeners.delete(jobId);
        resolve(result);
      });
    });
  }

  /**
   * Query job status from chain state
   */
  async getJobStatus(jobId: number): Promise<{
    status: 'Pending' | 'Running' | 'Completed' | 'Failed';
    result?: JobResult;
  }> {
    if (!this.api) throw new Error('Not connected to Tangle');

    const jobInfo = await this.api.query.jobs.jobs(jobId);

    if (jobInfo.isNone) {
      throw new Error(`Job ${jobId} not found`);
    }

    const job = jobInfo.unwrap();
    return this.decodeJobStatus(job);
  }

  /**
   * Submit batch of jobs for parallel execution
   */
  async submitBatchJobs(
    jobs: BlueprintJobArgs[],
    account: KeyringPair | string
  ): Promise<JobSubmissionResult[]> {
    if (!this.api) throw new Error('Not connected to Tangle');

    const signer = typeof account === 'string'
      ? this.keyring.addFromUri(account)
      : account;

    // Get current nonce
    const nonce = await this.api.rpc.system.accountNextIndex(signer.address);

    // Submit jobs with incremented nonces
    const submissions = jobs.map((args, index) =>
      this.submitJob(args, signer, { nonce: nonce.addn(index).toNumber() })
    );

    return Promise.all(submissions);
  }

  /**
   * Create operator for this blueprint
   */
  async registerOperator(
    account: KeyringPair | string,
    config: {
      bond_amount: bigint;
      service_endpoints: string[];
      capabilities: string[];
    }
  ): Promise<string> {
    if (!this.api) throw new Error('Not connected to Tangle');

    const signer = typeof account === 'string'
      ? this.keyring.addFromUri(account)
      : account;

    const extrinsic = this.api.tx.services.registerOperator(
      this.blueprintId,
      config.bond_amount,
      config.service_endpoints,
      config.capabilities
    );

    return new Promise((resolve, reject) => {
      extrinsic.signAndSend(signer, ({ status, dispatchError }) => {
        if (status.isFinalized) {
          if (dispatchError) {
            reject(new Error(this.decodeError(dispatchError)));
          } else {
            resolve(status.asFinalized.toString());
          }
        }
      }).catch(reject);
    });
  }

  /**
   * Query blueprint metadata
   */
  async getBlueprintInfo(): Promise<{
    id: number;
    name: string;
    version: string;
    operators: string[];
    job_count: number;
  }> {
    if (!this.api) throw new Error('Not connected to Tangle');

    const blueprintInfo = await this.api.query.services.blueprints(this.blueprintId);

    if (blueprintInfo.isNone) {
      throw new Error(`Blueprint ${this.blueprintId} not found`);
    }

    const blueprint = blueprintInfo.unwrap();
    return {
      id: this.blueprintId,
      name: blueprint.name.toString(),
      version: blueprint.version.toString(),
      operators: blueprint.operators.map((op: any) => op.toString()),
      job_count: blueprint.jobCount.toNumber(),
    };
  }

  // Private helper methods

  private encodeJobArgs(args: BlueprintJobArgs): Uint8Array {
    const encoded = {
      execution_mode: args.execution_mode,
      code: stringToHex(args.code),
      environment: args.environment,
      resource_requirements: args.resource_requirements || {
        cpu_cores: 1,
        memory_mb: 512,
        timeout_ms: 30000,
      },
      checkpoint_id: args.checkpoint_id || null,
      branch_from: args.branch_from || null,
    };

    return hexToU8a(stringToHex(JSON.stringify(encoded)));
  }

  private subscribeToEvents(): void {
    if (!this.api) return;

    this.api.query.system.events((events: EventRecord[]) => {
      events.forEach((record) => {
        const { event } = record;

        // Handle job completion events
        if (event.section === 'jobs' && event.method === 'JobCompleted') {
          const [blueprintId, jobId, resultData] = event.data;

          if (blueprintId.toNumber() === this.blueprintId) {
            const result = this.decodeJobResult(jobId.toNumber(), resultData);

            // Emit event
            this.emit('jobCompleted', result);

            // Notify any waiting listeners
            const listener = this.jobListeners.get(result.job_id);
            if (listener) {
              listener(result);
            }
          }
        }

        // Handle job failure events
        if (event.section === 'jobs' && event.method === 'JobFailed') {
          const [blueprintId, jobId, error] = event.data;

          if (blueprintId.toNumber() === this.blueprintId) {
            this.emit('jobFailed', {
              job_id: jobId.toNumber(),
              error: error.toString(),
            });
          }
        }
      });
    });
  }

  private decodeJobResult(jobId: number, resultData: any): JobResult {
    const decoded = JSON.parse(hexToU8a(resultData.toString()).toString());

    return {
      job_id: jobId,
      exit_code: decoded.exit_code,
      stdout: decoded.stdout,
      stderr: decoded.stderr,
      execution_time_ms: decoded.execution_time_ms,
      snapshot_id: decoded.snapshot_id,
      gas_used: BigInt(decoded.gas_used || 0),
    };
  }

  private decodeJobStatus(job: any): any {
    const status = job.status.toString();
    let result: JobResult | undefined;

    if (status === 'Completed' && job.result.isSome) {
      result = this.decodeJobResult(job.id.toNumber(), job.result.unwrap());
    }

    return { status, result };
  }

  private decodeError(dispatchError: any): string {
    if (!this.api) return 'Unknown error';

    if (dispatchError.isModule) {
      const decoded = this.api.registry.findMetaError(dispatchError.asModule);
      return `${decoded.section}.${decoded.name}: ${decoded.docs.join(' ')}`;
    } else {
      return dispatchError.toString();
    }
  }

  private getCustomTypes() {
    return {
      BlueprintId: 'u32',
      JobId: 'u64',
      OperatorId: 'AccountId',
      JobArgs: 'Vec<u8>',
      JobResult: {
        exit_code: 'i32',
        stdout: 'Vec<u8>',
        stderr: 'Vec<u8>',
        execution_time_ms: 'u64',
        snapshot_id: 'Option<Vec<u8>>',
        gas_used: 'u128',
      },
      JobStatus: {
        _enum: ['Pending', 'Running', 'Completed', 'Failed'],
      },
      Blueprint: {
        id: 'BlueprintId',
        name: 'Vec<u8>',
        version: 'Vec<u8>',
        operators: 'Vec<OperatorId>',
        jobCount: 'u64',
      },
    };
  }

  /**
   * Disconnect from Tangle network
   */
  async disconnect(): Promise<void> {
    if (this.api) {
      await this.api.disconnect();
      this.api = undefined;
      this.emit('disconnected');
    }
  }
}

/**
 * High-level Tangle job executor
 */
export class TangleJobExecutor {
  constructor(
    private client: TangleFaaSClient,
    private account: KeyringPair | string
  ) {}

  /**
   * Execute ephemeral job
   */
  async exec(code: string, env = 'alpine:latest'): Promise<JobResult> {
    const submission = await this.client.submitJob(
      {
        execution_mode: 'Ephemeral',
        code,
        environment: env,
      },
      this.account
    );

    return this.client.waitForJobResult(submission.job_id);
  }

  /**
   * Execute with checkpoint
   */
  async checkpoint(
    code: string,
    checkpointId?: string,
    env = 'alpine:latest'
  ): Promise<JobResult> {
    const submission = await this.client.submitJob(
      {
        execution_mode: 'Checkpointed',
        code,
        environment: env,
        checkpoint_id: checkpointId,
      },
      this.account
    );

    return this.client.waitForJobResult(submission.job_id);
  }

  /**
   * Branch execution
   */
  async branch(
    code: string,
    branchFrom: string,
    env = 'alpine:latest'
  ): Promise<JobResult> {
    const submission = await this.client.submitJob(
      {
        execution_mode: 'Branched',
        code,
        environment: env,
        branch_from: branchFrom,
      },
      this.account
    );

    return this.client.waitForJobResult(submission.job_id);
  }

  /**
   * Parallel map over items
   */
  async parallelMap<T, R>(
    items: T[],
    fn: (item: T) => string,
    env = 'alpine:latest'
  ): Promise<JobResult[]> {
    const jobs = items.map(item => ({
      execution_mode: 'Cached' as const,
      code: fn(item),
      environment: env,
    }));

    const submissions = await this.client.submitBatchJobs(jobs, this.account);

    return Promise.all(
      submissions.map(s => this.client.waitForJobResult(s.job_id))
    );
  }
}

// Export convenience functions
export async function connectToTangle(config?: TangleConfig): Promise<TangleFaaSClient> {
  const client = new TangleFaaSClient(config);
  await client.connect();
  return client;
}

export function createExecutor(
  client: TangleFaaSClient,
  account: KeyringPair | string
): TangleJobExecutor {
  return new TangleJobExecutor(client, account);
}