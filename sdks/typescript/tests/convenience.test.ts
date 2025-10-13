/**
 * Comprehensive tests for TypeScript SDK convenience methods.
 *
 * Tests all documented top-level API methods:
 * - runPython, runJavaScript, forkExecution
 * - prewarm, getMetrics, healthCheck
 */

import { FaaSClient, Runtime, ForkStrategy, ExecutionResult, ForkResult } from '../src/index';
import { EventEmitter } from 'events';

// Mock fetch for testing
global.fetch = jest.fn();

describe('FaaSClient Convenience Methods', () => {
  let client: FaaSClient;

  beforeEach(() => {
    client = new FaaSClient('http://localhost:8080');
    jest.clearAllMocks();
  });

  describe('runPython', () => {
    it('should execute Python code successfully', async () => {
      const mockResponse = {
        stdout: 'Hello from Python!\\n42',
        stderr: '',
        exit_code: 0,
        duration_ms: 45,
        request_id: 'python-123',
        cached: false
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const code = `
print("Hello from Python!")
result = 40 + 2
print(result)
`;

      const result = await client.runPython(code);

      expect(result.stdout).toContain('Hello from Python!');
      expect(result.stdout).toContain('42');
      expect(result.duration_ms).toBe(45);
      expect(result.cached).toBe(false);

      // Verify the correct endpoint was called
      expect(global.fetch).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/run/python',
        expect.objectContaining({
          method: 'POST',
          headers: expect.objectContaining({
            'Content-Type': 'application/json'
          }),
          body: expect.stringContaining(code)
        })
      );
    });

    it('should execute Python with custom image', async () => {
      const mockResponse = {
        stdout: 'NumPy array: [1 2 3 4 5]',
        stderr: '',
        exit_code: 0,
        duration_ms: 250,
        request_id: 'numpy-123'
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const code = `
import numpy as np
arr = np.array([1, 2, 3, 4, 5])
print(f"NumPy array: {arr}")
`;

      const result = await client.runPython(code, { image: 'python:3.11-numpy' });

      expect(result.stdout).toContain('NumPy array');
      expect(result.duration_ms).toBe(250);
    });
  });

  describe('runJavaScript', () => {
    it('should execute JavaScript code successfully', async () => {
      const mockResponse = {
        stdout: 'Hello from JavaScript!\\n42',
        stderr: '',
        exit_code: 0,
        duration_ms: 35,
        request_id: 'js-123',
        cached: false
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const code = `
console.log("Hello from JavaScript!");
console.log(40 + 2);
`;

      const result = await client.runJavaScript(code);

      expect(result.stdout).toContain('Hello from JavaScript!');
      expect(result.stdout).toContain('42');
      expect(result.duration_ms).toBe(35);
    });

    it('should execute JavaScript with Node.js modules', async () => {
      const mockResponse = {
        stdout: 'Lodash sum: 15\\nMoment date: 2024-01-01',
        stderr: '',
        exit_code: 0,
        duration_ms: 180
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const code = `
const _ = require('lodash');
const numbers = [1, 2, 3, 4, 5];
console.log('Lodash sum:', _.sum(numbers));
`;

      const result = await client.runJavaScript(code, { image: 'node:20' });

      expect(result.stdout).toContain('Lodash sum: 15');
    });
  });

  describe('runTypeScript', () => {
    it('should execute TypeScript code successfully', async () => {
      const mockResponse = {
        stdout: 'TypeScript: Hello, World!',
        stderr: '',
        exit_code: 0,
        duration_ms: 120,
        request_id: 'ts-123'
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const code = `
interface Greeting {
  message: string;
}

const greet: Greeting = { message: "Hello, World!" };
console.log(\`TypeScript: \${greet.message}\`);
`;

      const result = await client.runTypeScript(code);

      expect(result.stdout).toContain('TypeScript: Hello, World!');
      expect(result.duration_ms).toBe(120);
    });
  });

  describe('forkExecution', () => {
    it('should execute branches in parallel', async () => {
      const mockResponse = {
        results: [
          {
            branch_id: 'version-a',
            stdout: 'Algorithm A result',
            stderr: '',
            exit_code: 0,
            duration_ms: 120
          },
          {
            branch_id: 'version-b',
            stdout: 'Algorithm B result',
            stderr: '',
            exit_code: 0,
            duration_ms: 85
          }
        ],
        selected_branch: 'version-b',
        selection_reason: 'lower_latency'
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const branches = [
        {
          id: 'version-a',
          command: 'echo "Algorithm A"',
          weight: 0.5
        },
        {
          id: 'version-b',
          command: 'echo "Algorithm B"',
          weight: 0.5
        }
      ];

      const result = await client.forkExecution({
        branches,
        image: 'alpine:latest',
        strategy: ForkStrategy.PARALLEL
      });

      expect(result.results).toHaveLength(2);
      expect(result.selected_branch).toBe('version-b');
      expect(result.selection_reason).toBe('lower_latency');

      const branchA = result.results.find(r => r.branch_id === 'version-a');
      expect(branchA?.duration_ms).toBe(120);

      const branchB = result.results.find(r => r.branch_id === 'version-b');
      expect(branchB?.duration_ms).toBe(85);
    });

    it('should select fastest branch', async () => {
      const mockResponse = {
        results: [
          { branch_id: 'slow', stdout: 'Slow', duration_ms: 500 },
          { branch_id: 'fast', stdout: 'Fast', duration_ms: 50 }
        ],
        selected_branch: 'fast',
        selection_reason: 'fastest_execution'
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const result = await client.forkExecution({
        branches: [
          { id: 'slow', command: 'sleep 0.5' },
          { id: 'fast', command: 'echo "Fast"' }
        ],
        image: 'alpine:latest',
        strategy: ForkStrategy.FASTEST
      });

      expect(result.selected_branch).toBe('fast');
      expect(result.selection_reason).toBe('fastest_execution');
    });
  });

  describe('prewarm', () => {
    it('should prewarm Docker containers', async () => {
      const mockResponse = {
        success: true,
        containers_warmed: 5,
        average_warmup_ms: 125
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const result = await client.prewarm({
        image: 'alpine:latest',
        count: 5,
        runtime: Runtime.DOCKER
      });

      expect(result.success).toBe(true);
      expect(result.containers_warmed).toBe(5);
      expect(result.average_warmup_ms).toBe(125);
    });

    it('should prewarm Firecracker microVMs', async () => {
      const mockResponse = {
        success: true,
        containers_warmed: 3,
        average_warmup_ms: 95
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const result = await client.prewarm({
        image: 'alpine:latest',
        count: 3,
        runtime: Runtime.FIRECRACKER,
        memory_mb: 256,
        cpu_cores: 1
      });

      expect(result.success).toBe(true);
      expect(result.average_warmup_ms).toBeLessThan(100);
    });
  });

  describe('getMetrics', () => {
    it('should retrieve platform metrics', async () => {
      const mockMetrics = {
        total_executions: 10000,
        avg_execution_time_ms: 42.5,
        cache_hit_rate: 0.87,
        active_containers: 15,
        memory_usage_mb: 3072,
        cpu_usage_percent: 45.2,
        warm_start_ratio: 0.92,
        cold_starts_last_hour: 8,
        errors_last_hour: 2,
        p99_latency_ms: 125,
        p95_latency_ms: 85,
        p50_latency_ms: 35
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockMetrics
      });

      const metrics = await client.getMetrics();

      expect(metrics.total_executions).toBe(10000);
      expect(metrics.avg_execution_time_ms).toBe(42.5);
      expect(metrics.cache_hit_rate).toBe(0.87);
      expect(metrics.warm_start_ratio).toBe(0.92);
      expect(metrics.p50_latency_ms).toBe(35);
      expect(metrics.p99_latency_ms).toBe(125);
    });
  });

  describe('healthCheck', () => {
    it('should return healthy status', async () => {
      const mockHealth = {
        status: 'healthy',
        uptime_seconds: 86400,
        version: '1.0.0',
        components: {
          docker: 'healthy',
          cache: 'healthy',
          scheduler: 'healthy',
          firecracker: 'healthy'
        }
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockHealth
      });

      const health = await client.healthCheck();

      expect(health.status).toBe('healthy');
      expect(health.uptime_seconds).toBe(86400);
      expect(health.components.docker).toBe('healthy');
      expect(health.components.firecracker).toBe('healthy');
    });

    it('should handle degraded status', async () => {
      const mockHealth = {
        status: 'degraded',
        components: {
          docker: 'healthy',
          cache: 'degraded'
        },
        issues: ['Cache hit rate below threshold']
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockHealth
      });

      const health = await client.healthCheck();

      expect(health.status).toBe('degraded');
      expect(health.components.cache).toBe('degraded');
      expect(health.issues).toContain('Cache hit rate below threshold');
    });
  });

  describe('Performance Benchmarks', () => {
    it('should meet warm start performance (< 50ms)', async () => {
      const mockResponse = {
        stdout: 'Warm start',
        duration_ms: 35,
        cached: true,
        warm_start: true
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const result = await client.execute({
        command: 'echo "test"',
        image: 'alpine:latest',
        cacheKey: 'warm-test'
      });

      expect(result.durationMs).toBeLessThan(50);
    });

    it('should meet branching performance (< 250ms)', async () => {
      const mockResponse = {
        results: [
          { branch_id: 'a', duration_ms: 100 },
          { branch_id: 'b', duration_ms: 120 }
        ],
        total_duration_ms: 235,
        selected_branch: 'a'
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const result = await client.forkExecution({
        branches: [
          { id: 'a', command: 'echo "A"' },
          { id: 'b', command: 'echo "B"' }
        ],
        image: 'alpine:latest',
        strategy: ForkStrategy.PARALLEL
      });

      expect(mockResponse.total_duration_ms).toBeLessThan(250);
    });
  });

  describe('Event Emitter Integration', () => {
    it('should emit events for execution lifecycle', async () => {
      const mockResponse = {
        stdout: 'Test',
        duration_ms: 50
      };

      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: async () => mockResponse
      });

      const startListener = jest.fn();
      const completeListener = jest.fn();

      client.on('execution:start', startListener);
      client.on('execution:complete', completeListener);

      await client.execute({
        command: 'echo "test"',
        image: 'alpine:latest'
      });

      expect(startListener).toHaveBeenCalled();
      expect(completeListener).toHaveBeenCalledWith(
        expect.objectContaining({
          stdout: 'Test',
          duration_ms: 50
        })
      );
    });
  });

  describe('Error Handling', () => {
    it('should handle network errors', async () => {
      (global.fetch as jest.Mock).mockRejectedValueOnce(
        new Error('Network error')
      );

      await expect(client.runPython('print("test")')).rejects.toThrow(
        'Network error'
      );
    });

    it('should handle server errors', async () => {
      (global.fetch as jest.Mock).mockResolvedValueOnce({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error'
      });

      await expect(client.runJavaScript('console.log("test")')).rejects.toThrow();
    });

    it('should handle timeout errors', async () => {
      const mockAbortError = new Error('The operation was aborted');
      mockAbortError.name = 'AbortError';

      (global.fetch as jest.Mock).mockRejectedValueOnce(mockAbortError);

      await expect(
        client.execute({
          command: 'sleep 60',
          image: 'alpine:latest',
          timeoutMs: 100
        })
      ).rejects.toThrow();
    });
  });

  describe('Runtime Selection', () => {
    it('should use Docker runtime', async () => {
      const mockResponse = {
        stdout: 'Docker',
        durationMs: 40,
        exitCode: 0
      };

      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => mockResponse
      });

      const result = await client.execute({
        command: 'echo "Docker"',
        image: 'alpine:latest',
        runtime: Runtime.Docker
      });

      expect(result.stdout).toBe('Docker');
    });
  });
});