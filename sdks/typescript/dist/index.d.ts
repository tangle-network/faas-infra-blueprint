/**
 * # FaaS Platform TypeScript SDK
 *
 * Official TypeScript/JavaScript SDK for the FaaS Platform, providing high-performance
 * serverless execution with support for both Docker containers and Firecracker microVMs.
 *
 * ## Key Features
 *
 * - **🚀 Dual Runtime Support**: Choose Docker for development, Firecracker for production
 * - **📊 Intelligent Caching**: Automatic result caching with configurable TTL
 * - **🔥 Pre-warming**: Zero cold starts with warm container pools
 * - **🌳 Execution Forking**: Branch workflows for A/B testing and parallel execution
 * - **📈 Auto-scaling**: Predictive scaling based on load patterns
 * - **📋 Rich Metrics**: Built-in performance monitoring with event emitters
 * - **🔄 Method Chaining**: Fluent API design for easy configuration
 *
 * ## Quick Start
 *
 * ```typescript
 * import { FaaSClient, Runtime } from '@faas-platform/sdk';
 *
 * const client = new FaaSClient('http://localhost:8080');
 *
 * // Simple execution
 * const result = await client.runJavaScript('console.log("Hello, World!")');
 * console.log(result.output); // Output: Hello, World!
 *
 * // With specific runtime
 * const prodResult = await client
 *   .useFirecracker()
 *   .setCaching(true)
 *   .execute({ command: 'python inference.py' });
 * ```
 *
 * ## Performance Characteristics
 *
 * | Runtime | Cold Start | Security | Best For |
 * |---------|------------|----------|----------|
 * | Docker | 50-200ms | Process isolation | Development, testing |
 * | Firecracker | ~125ms | Hardware isolation | Production, multi-tenant |
 * | Auto | Varies | Adaptive | Automatic selection |
 *
 * ## Runtime Selection Guide
 *
 * **Docker Containers:**
 * - ✅ Fastest cold starts (50-200ms)
 * - ✅ Rich ecosystem and GPU support
 * - ✅ Hot reload for development
 * - ❌ Process-level isolation only
 *
 * **Firecracker microVMs:**
 * - ✅ Hardware-level isolation
 * - ✅ Memory encryption and compliance-ready
 * - ✅ Multi-tenant safe
 * - ❌ Linux only, limited GPU support
 *
 * @packageDocumentation
 * @version 1.0.0
 * @author FaaS Platform Team
 * @since 1.0.0
 */
import { EventEmitter } from 'events';
/**
 * Execution runtime environment selection.
 *
 * Choose the optimal runtime based on your specific requirements for
 * performance, security, and isolation.
 *
 * @example
 * ```typescript
 * import { FaaSClient, Runtime } from '@faas-platform/sdk';
 *
 * // Development with Docker (fastest iteration)
 * const devClient = new FaaSClient('http://localhost:8080', {
 *   runtime: Runtime.Docker
 * });
 *
 * // Production with Firecracker (strongest security)
 * const prodClient = new FaaSClient('https://api.example.com', {
 *   runtime: Runtime.Firecracker
 * });
 *
 * // Automatic selection (platform decides)
 * const smartClient = new FaaSClient('http://localhost:8080', {
 *   runtime: Runtime.Auto
 * });
 * ```
 */
export declare enum Runtime {
    /**
     * Docker containers - optimal for development and testing.
     *
     * **Performance:**
     * - Cold start: 50-200ms
     * - Hot reload support
     * - Excellent for rapid iteration
     *
     * **Features:**
     * - GPU passthrough support
     * - Rich container ecosystem
     * - Native development experience
     *
     * **Limitations:**
     * - Process-level isolation only
     * - Shared kernel with host
     */
    Docker = "docker",
    /**
     * Firecracker microVMs - optimal for production and multi-tenant environments.
     *
     * **Performance:**
     * - Cold start: ~125ms
     * - Hardware-level isolation
     * - Memory encryption support
     *
     * **Features:**
     * - Complete kernel isolation
     * - Compliance and audit-ready
     * - Multi-tenant security
     *
     * **Limitations:**
     * - Linux environments only
     * - Limited GPU support
     */
    Firecracker = "firecracker",
    /**
     * Automatic runtime selection based on workload analysis.
     *
     * The platform analyzes your workload characteristics and automatically
     * selects the optimal runtime considering:
     * - Security requirements
     * - Performance constraints
     * - Resource availability
     * - Historical execution patterns
     *
     * **Use Cases:**
     * - Mixed workload environments
     * - Gradual migration scenarios
     * - Dynamic optimization needs
     */
    Auto = "auto"
}
/**
 * Function execution mode for advanced workflow control.
 *
 * Different execution modes provide varying levels of state management,
 * caching, and persistence to optimize for different use cases.
 *
 * @example
 * ```typescript
 * // Cached ML inference for repeated requests
 * const result = await client.executeAdvanced({
 *   command: 'python inference.py',
 *   mode: ExecutionMode.Cached,
 *   image: 'pytorch/pytorch:latest'
 * });
 *
 * // Persistent service that maintains state
 * const service = await client.executeAdvanced({
 *   command: 'node server.js',
 *   mode: ExecutionMode.Persistent,
 *   image: 'node:20-alpine'
 * });
 * ```
 */
export declare enum ExecutionMode {
    /**
     * One-time execution with no state persistence.
     *
     * - Default mode for simple executions
     * - Minimal resource overhead
     * - Clean slate for each execution
     * - No caching or state retention
     */
    Ephemeral = "ephemeral",
    /**
     * Execution with intelligent result caching.
     *
     * - Results cached based on input hash
     * - Subsequent identical requests return cached results instantly
     * - Configurable TTL for cache expiration
     * - Perfect for deterministic computations
     */
    Cached = "cached",
    /**
     * Execution with state checkpoint/restore capabilities.
     *
     * - State can be saved at any point during execution
     * - Enables pause/resume workflows
     * - Ideal for long-running computations
     * - Fault tolerance through state snapshots
     */
    Checkpointed = "checkpointed",
    /**
     * Execution that supports forking into multiple paths.
     *
     * - Parent state can spawn multiple child executions
     * - Copy-on-write memory optimization
     * - Perfect for A/B testing and parallel workflows
     * - Each fork operates independently
     */
    Branched = "branched",
    /**
     * Long-running execution with persistent state.
     *
     * - Container/VM stays alive between requests
     * - Maintains in-memory state and connections
     * - Optimal for stateful services and databases
     * - Connection pooling and session management
     */
    Persistent = "persistent"
}
/**
 * Result from function execution
 */
export interface ExecutionResult {
    requestId: string;
    output?: string;
    logs?: string;
    error?: string;
    durationMs: number;
    cacheHit: boolean;
    runtimeUsed?: Runtime;
}
/**
 * Client configuration options
 */
export interface ClientConfig {
    baseUrl: string;
    runtime?: Runtime;
    cacheEnabled?: boolean;
    maxRetries?: number;
    timeout?: number;
    apiKey?: string;
}
/**
 * Client-side performance metrics
 */
export declare class ClientMetrics {
    totalRequests: number;
    cacheHits: number;
    totalLatencyMs: number;
    errors: number;
    get cacheHitRate(): number;
    get averageLatencyMs(): number;
    get errorRate(): number;
}
/**
 * FaaS Platform Client
 *
 * @example
 * ```typescript
 * const client = new FaaSClient('http://localhost:8080');
 * const result = await client.runPython('print("Hello, World!")');
 * console.log(result.output);
 * ```
 */
/**
 * High-performance FaaS Platform client with intelligent optimization and event-driven architecture.
 *
 * The FaaSClient provides a unified interface for executing code on both Docker containers
 * and Firecracker microVMs, featuring automatic optimization, caching, scaling, and rich
 * event-driven monitoring capabilities.
 *
 * @extends EventEmitter
 *
 * @example
 * Basic usage:
 * ```typescript
 * import { FaaSClient } from '@faas-platform/sdk';
 *
 * const client = new FaaSClient('http://localhost:8080');
 *
 * // Simple JavaScript execution
 * const result = await client.runJavaScript('console.log("Hello, World!")');
 * console.log(result.output); // Output: Hello, World!
 *
 * // Advanced execution with options
 * const mlResult = await client.execute({
 *   command: 'python inference.py',
 *   image: 'pytorch/pytorch:latest',
 *   envVars: { MODEL_PATH: '/models/bert' },
 *   runtime: Runtime.Firecracker,
 *   timeoutMs: 60000
 * });
 * ```
 *
 * @example
 * Method chaining for configuration:
 * ```typescript
 * const client = new FaaSClient('http://localhost:8080')
 *   .useFirecracker()        // Use Firecracker VMs
 *   .setCaching(true)        // Enable result caching
 *   .setRetries(5);          // Set retry attempts
 *
 * const result = await client.execute({
 *   command: 'node process.js'
 * });
 * ```
 *
 * @example
 * Event-driven monitoring:
 * ```typescript
 * const client = new FaaSClient('http://localhost:8080');
 *
 * // Listen to execution events
 * client.on('execution', (event) => {
 *   console.log(`Execution completed in ${event.elapsedMs}ms`);
 *   console.log(`Cache hit: ${event.cacheHit}`);
 * });
 *
 * // Listen to retry events
 * client.on('retry', (event) => {
 *   console.log(`Retry attempt ${event.attempt}: ${event.error}`);
 * });
 *
 * // Listen to error events
 * client.on('error', (error) => {
 *   console.error('Client error:', error);
 * });
 *
 * await client.runPython('print("Monitoring events!")');
 * ```
 *
 * @example
 * Advanced workflow orchestration:
 * ```typescript
 * // Pre-warm containers for zero cold start
 * await client.prewarm('node:20-alpine', 5);
 *
 * // Execute with forking for A/B testing
 * const baseExecution = await client.execute({
 *   command: 'node setup.js'
 * });
 *
 * const [variantA, variantB] = await Promise.all([
 *   client.forkExecution(baseExecution.requestId, 'node variant-a.js'),
 *   client.forkExecution(baseExecution.requestId, 'node variant-b.js')
 * ]);
 *
 * // Create snapshots for state management
 * const snapshot = await client.createSnapshot({
 *   name: 'checkpoint-1',
 *   containerId: baseExecution.requestId,
 *   description: 'State after initialization'
 * });
 * ```
 *
 * @example
 * Production configuration with full observability:
 * ```typescript
 * import { FaaSClient, Runtime, ClientConfig } from '@faas-platform/sdk';
 *
 * const config: ClientConfig = {
 *   baseUrl: 'https://api.faas-platform.com',
 *   runtime: Runtime.Firecracker,
 *   cacheEnabled: true,
 *   maxRetries: 3,
 *   timeout: 30000,
 *   apiKey: process.env.FAAS_API_KEY
 * };
 *
 * const client = new FaaSClient(config);
 *
 * // Monitor performance metrics
 * setInterval(async () => {
 *   const serverMetrics = await client.getMetrics();
 *   const clientMetrics = client.getClientMetrics();
 *
 *   console.log('Server metrics:', serverMetrics);
 *   console.log('Client cache hit rate:', clientMetrics.cacheHitRate);
 * }, 30000);
 * ```
 *
 * ## Events
 *
 * The client emits the following events:
 *
 * - `execution`: Fired when an execution completes
 * - `retry`: Fired when a request is retried
 * - `error`: Fired when an unrecoverable error occurs
 * - `cache-hit`: Fired when a cached result is returned
 * - `cache-miss`: Fired when no cache entry is found
 *
 * ## Performance Tips
 *
 * - **Use method chaining** for configuration to avoid creating multiple instances
 * - **Enable caching** for deterministic computations to achieve <10ms response times
 * - **Pre-warm containers** for critical paths to eliminate cold starts
 * - **Use Firecracker** for production workloads requiring isolation
 * - **Monitor events** for performance insights and debugging
 * - **Batch operations** when possible to reduce network overhead
 *
 * ## Thread Safety
 *
 * FaaSClient is fully thread-safe and can be shared across multiple concurrent operations.
 * The client uses connection pooling and automatically manages request queuing.
 *
 * @since 1.0.0
 * @public
 */
export declare class FaaSClient extends EventEmitter {
    private config;
    private client;
    private metrics;
    constructor(config: ClientConfig | string);
    /**
     * Generate cache key from content
     */
    private getCacheKey;
    /**
     * Execute with retry logic
     */
    private executeWithRetry;
    /**
     * Execute a command in a container or VM
     */
    execute(options: {
        command: string;
        image?: string;
        runtime?: Runtime;
        envVars?: Record<string, string>;
        workingDir?: string;
        timeoutMs?: number;
        cacheKey?: string;
    }): Promise<ExecutionResult>;
    /**
     * Execute Python code directly
     */
    runPython(code: string, options?: Partial<Parameters<typeof this.execute>[0]>): Promise<ExecutionResult>;
    /**
     * Execute JavaScript/Node.js code with automatic environment setup.
     *
     * This convenience method runs JavaScript code in a pre-configured Node.js runtime
     * with common packages available and proper error handling.
     *
     * @param code - JavaScript source code to execute
     * @param options - Optional execution parameters (image, envVars, etc.)
     * @returns Promise resolving to execution result with output and metadata
     *
     * @example
     * Basic JavaScript execution:
     * ```typescript
     * const result = await client.runJavaScript(`
     *   const fs = require('fs');
     *   const data = [1, 2, 3, 4, 5];
     *   const sum = data.reduce((a, b) => a + b, 0);
     *   console.log('Sum:', sum);
     * `);
     * console.log(result.output); // Output: Sum: 15
     * ```
     *
     * @example
     * With custom environment and packages:
     * ```typescript
     * const result = await client.runJavaScript(`
     *   const axios = require('axios');
     *
     *   async function fetchData() {
     *     const response = await axios.get('https://api.example.com/data');
     *     console.log('Data:', response.data);
     *   }
     *
     *   fetchData().catch(console.error);
     * `, {
     *   image: 'node:20-with-deps', // Custom image with axios pre-installed
     *   envVars: { API_KEY: process.env.API_KEY }
     * });
     * ```
     *
     * @example
     * Error handling:
     * ```typescript
     * try {
     *   const result = await client.runJavaScript('throw new Error("Test error")');
     *   if (result.error) {
     *     console.log('JavaScript error:', result.error);
     *   }
     * } catch (clientError) {
     *   console.error('Client error:', clientError);
     * }
     * ```
     */
    runJavaScript(code: string, options?: Partial<Parameters<typeof this.execute>[0]>): Promise<ExecutionResult>;
    /**
     * Execute TypeScript code (transpiled)
     */
    runTypeScript(code: string, options?: Partial<Parameters<typeof this.execute>[0]>): Promise<ExecutionResult>;
    /**
     * Execute Bash script
     */
    runBash(script: string, options?: Partial<Parameters<typeof this.execute>[0]>): Promise<ExecutionResult>;
    /**
     * Fork execution from a parent for A/B testing
     */
    forkExecution(parentId: string, command: string, options?: Partial<Parameters<typeof this.execute>[0]>): Promise<ExecutionResult>;
    /**
     * Create a snapshot of a container
     */
    createSnapshot(containerId: string, name: string, description?: string): Promise<any>;
    /**
     * Pre-warm containers for zero cold starts
     */
    prewarm(image: string, count?: number): Promise<void>;
    /**
     * Stream logs from an execution
     */
    streamLogs(executionId: string): AsyncGenerator<string, void, unknown>;
    /**
     * Get server-side performance metrics
     */
    getMetrics(): Promise<any>;
    /**
     * Get client-side metrics
     */
    getClientMetrics(): ClientMetrics;
    /**
     * Check platform health status
     */
    healthCheck(): Promise<any>;
    /**
     * Use Docker runtime
     */
    useDocker(): this;
    /**
     * Use Firecracker VM runtime
     */
    useFirecracker(): this;
    /**
     * Enable/disable caching
     */
    setCaching(enabled: boolean): this;
}
/**
 * Function builder for complex configurations
 *
 * @example
 * ```typescript
 * const func = new FunctionBuilder('my-function')
 *   .runtime(Runtime.Firecracker)
 *   .withEnv('API_KEY', 'secret')
 *   .withMemory(512)
 *   .build();
 * ```
 */
export declare class FunctionBuilder {
    private config;
    constructor(name: string);
    runtime(runtime: Runtime): this;
    withEnv(key: string, value: string): this;
    withMemory(mb: number): this;
    withCPU(cores: number): this;
    withTimeout(ms: number): this;
    build(): any;
}
/**
 * Quick execution helper
 */
export declare function run(code: string, language?: 'python' | 'javascript' | 'typescript' | 'bash', baseUrl?: string): Promise<string>;
export default FaaSClient;
//# sourceMappingURL=index.d.ts.map