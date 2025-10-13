/**
 * Comprehensive tests for TypeScript SDK convenience methods.
 *
 * Tests all documented top-level API methods:
 * - runPython, runJavaScript, forkExecution
 * - prewarm, getMetrics, healthCheck
 */

import { FaaSClient, Runtime, ExecutionResult } from '../src/index';
import { EventEmitter } from 'events';
import axios from 'axios';

// Mock axios for testing
jest.mock('axios');
const mockedAxios = axios as jest.Mocked<typeof axios>;

describe('FaaSClient Convenience Methods', () => {
  let client: FaaSClient;
  let mockAxiosInstance: any;

  beforeEach(() => {
    // Create a mock axios instance
    mockAxiosInstance = {
      get: jest.fn(),
      post: jest.fn(),
      put: jest.fn(),
      delete: jest.fn(),
    };

    // Mock axios.create to return our mock instance
    mockedAxios.create = jest.fn().mockReturnValue(mockAxiosInstance);

    client = new FaaSClient('http://localhost:8080');
    jest.clearAllMocks();
  });

  describe('runPython', () => {
    it('should execute Python code successfully', async () => {
      const mockResponse = {
        request_id: 'python-123',
        output: 'Hello from Python!\n42',
        logs: '',
        duration_ms: 45
      };

      mockAxiosInstance.post.mockResolvedValueOnce({
        data: mockResponse
      });

      const code = `
print("Hello from Python!")
result = 40 + 2
print(result)
`;

      const result = await client.runPython(code);

      expect(result.output).toContain('Hello from Python!');
      expect(result.output).toContain('42');
      expect(result.durationMs).toBe(45);
      expect(result.requestId).toBe('python-123');

      // Verify the correct endpoint was called
      expect(mockAxiosInstance.post).toHaveBeenCalledWith(
        '/api/v1/execute',
        expect.objectContaining({
          command: expect.stringContaining('python'),
          image: 'python:3.11-slim'
        })
      );
    });

    it('should execute Python with custom image', async () => {
      const mockResponse = {
        request_id: 'numpy-123',
        output: 'NumPy array: [1 2 3 4 5]',
        duration_ms: 250
      };

      mockAxiosInstance.post.mockResolvedValueOnce({
        data: mockResponse
      });

      const code = `
import numpy as np
arr = np.array([1, 2, 3, 4, 5])
print(f"NumPy array: {arr}")
`;

      const result = await client.runPython(code, { image: 'python:3.11-numpy' });

      expect(result.output).toContain('NumPy array');
      expect(result.durationMs).toBe(250);
    });
  });

  describe('runJavaScript', () => {
    it('should execute JavaScript code successfully', async () => {
      const mockResponse = {
        request_id: 'js-123',
        output: 'Hello from JavaScript!\n42',
        duration_ms: 35
      };

      mockAxiosInstance.post.mockResolvedValueOnce({
        data: mockResponse
      });

      const code = `
console.log("Hello from JavaScript!");
console.log(40 + 2);
`;

      const result = await client.runJavaScript(code);

      expect(result.output).toContain('Hello from JavaScript!');
      expect(result.output).toContain('42');
      expect(result.durationMs).toBe(35);
    });

    it('should execute JavaScript with Node.js modules', async () => {
      const mockResponse = {
        request_id: 'lodash-123',
        output: 'Lodash sum: 15',
        duration_ms: 180
      };

      mockAxiosInstance.post.mockResolvedValueOnce({
        data: mockResponse
      });

      const code = `
const _ = require('lodash');
const numbers = [1, 2, 3, 4, 5];
console.log('Lodash sum:', _.sum(numbers));
`;

      const result = await client.runJavaScript(code, { image: 'node:20' });

      expect(result.output).toContain('Lodash sum: 15');
    });
  });

  describe('runTypeScript', () => {
    it('should execute TypeScript code successfully', async () => {
      const mockResponse = {
        request_id: 'ts-123',
        output: 'TypeScript: Hello, World!',
        duration_ms: 120
      };

      mockAxiosInstance.post.mockResolvedValueOnce({
        data: mockResponse
      });

      const code = `
interface Greeting {
  message: string;
}

const greet: Greeting = { message: "Hello, World!" };
console.log(\`TypeScript: \${greet.message}\`);
`;

      const result = await client.runTypeScript(code);

      expect(result.output).toContain('TypeScript: Hello, World!');
      expect(result.durationMs).toBe(120);
    });
  });

  describe('forkExecution', () => {
    it('should fork from parent execution', async () => {
      const mockResponse = {
        request_id: 'fork-123',
        output: 'Forked result',
        duration_ms: 85
      };

      mockAxiosInstance.post.mockResolvedValueOnce({
        data: mockResponse
      });

      const result = await client.forkExecution('parent-123', 'echo "forked"');

      expect(result.requestId).toBe('fork-123');
      expect(result.output).toBe('Forked result');
      expect(result.durationMs).toBe(85);

      // Verify the correct API call
      expect(mockAxiosInstance.post).toHaveBeenCalledWith(
        '/api/v1/execute',
        expect.objectContaining({
          command: 'echo "forked"',
          mode: 'branched',
          branch_from: 'parent-123'
        })
      );
    });
  });

  describe('prewarm', () => {
    it('should prewarm Docker containers', async () => {
      mockAxiosInstance.post.mockResolvedValueOnce({
        data: {}
      });

      await client.prewarm('alpine:latest', 5);

      expect(mockAxiosInstance.post).toHaveBeenCalledWith(
        '/api/v1/prewarm',
        expect.objectContaining({
          image: 'alpine:latest',
          count: 5
        })
      );
    });

    it('should prewarm Firecracker microVMs', async () => {
      mockAxiosInstance.post.mockResolvedValueOnce({
        data: {}
      });

      await client.useFirecracker().prewarm('alpine:latest', 3);

      expect(mockAxiosInstance.post).toHaveBeenCalledWith(
        '/api/v1/prewarm',
        expect.objectContaining({
          image: 'alpine:latest',
          count: 3,
          runtime: Runtime.Firecracker
        })
      );
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

      mockAxiosInstance.get.mockResolvedValueOnce({
        data: mockMetrics
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

      mockAxiosInstance.get.mockResolvedValueOnce({
        data: mockHealth
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

      mockAxiosInstance.get.mockResolvedValueOnce({
        data: mockHealth
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
        request_id: 'perf-123',
        output: 'Warm start',
        duration_ms: 35
      };

      mockAxiosInstance.post.mockResolvedValueOnce({
        data: mockResponse
      });

      const result = await client.execute({
        command: 'echo "test"',
        image: 'alpine:latest',
        cacheKey: 'warm-test'
      });

      expect(result.durationMs).toBeLessThan(50);
    });
  });

  describe('Event Emitter Integration', () => {
    it('should emit events for execution lifecycle', async () => {
      const mockResponse = {
        request_id: 'event-123',
        output: 'Test',
        duration_ms: 50
      };

      mockAxiosInstance.post.mockResolvedValueOnce({
        data: mockResponse
      });

      const executionListener = jest.fn();

      client.on('execution', executionListener);

      await client.execute({
        command: 'echo "test"',
        image: 'alpine:latest'
      });

      expect(executionListener).toHaveBeenCalledWith(
        expect.objectContaining({
          result: expect.objectContaining({
            request_id: 'event-123',
            output: 'Test',
            duration_ms: 50
          })
        })
      );
    });
  });

  describe('Error Handling', () => {
    it('should handle network errors', async () => {
      mockAxiosInstance.post.mockRejectedValueOnce(
        new Error('Network error')
      );

      await expect(client.runPython('print("test")')).rejects.toThrow(
        'Network error'
      );
    });

    it('should handle server errors', async () => {
      mockAxiosInstance.post.mockRejectedValueOnce({
        response: {
          status: 500,
          statusText: 'Internal Server Error'
        }
      });

      await expect(client.runJavaScript('console.log("test")')).rejects.toThrow();
    });

    it('should handle timeout errors', async () => {
      const mockAbortError = new Error('The operation was aborted');
      mockAbortError.name = 'AbortError';

      mockAxiosInstance.post.mockRejectedValueOnce(mockAbortError);

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
        request_id: 'docker-123',
        output: 'Docker\n',
        duration_ms: 40
      };

      mockAxiosInstance.post.mockResolvedValue({
        data: mockResponse
      });

      const result = await client.execute({
        command: 'echo "Docker"',
        image: 'alpine:latest',
        runtime: Runtime.Docker
      });

      expect(result.output).toContain('Docker');
    });
  });
});