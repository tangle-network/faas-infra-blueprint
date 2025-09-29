/**
 * Comprehensive test suite for FaaS TypeScript SDK
 *
 * Tests all documented API methods:
 * - execute, runPython, runJavaScript, runBash
 * - forkExecution, prewarm, getMetrics, healthCheck
 */

import { FaaSClient, Runtime, ExecutionResult, ExecutionMode } from '../src/index';
import fetchMock from 'jest-fetch-mock';

// Enable fetch mocking
fetchMock.enableMocks();

describe('FaaS TypeScript SDK', () => {
  let client: FaaSClient;

  beforeEach(() => {
    fetchMock.resetMocks();
    client = new FaaSClient('http://localhost:8080');
  });

  describe('execute', () => {
    it('should execute basic commands successfully', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        stdout: 'Hello from FaaS!',
        stderr: '',
        exit_code: 0,
        duration_ms: 45,
        request_id: 'test-123'
      }));

      const result = await client.execute({
        command: "echo 'Hello from FaaS!'",
        image: 'alpine:latest'
      });

      expect(result.output).toBe('Hello from FaaS!');
      expect(result.exitCode).toBe(0);
      expect(result.durationMs).toBe(45);
      expect(fetchMock).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/execute',
        expect.objectContaining({
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: expect.stringContaining('Hello from FaaS!')
        })
      );
    });

    it('should handle working directory parameter', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        stdout: '/app',
        stderr: '',
        exit_code: 0,
        duration_ms: 20,
        request_id: 'workdir-test'
      }));

      const result = await client.execute({
        command: 'pwd',
        workingDir: '/app'
      });

      expect(result.output).toBe('/app');
      expect(fetchMock).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/execute',
        expect.objectContaining({
          body: expect.stringContaining('/app')
        })
      );
    });

    it('should handle environment variables', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        stdout: 'TEST_VAR=production',
        stderr: '',
        exit_code: 0,
        duration_ms: 35,
        request_id: 'env-test'
      }));

      const result = await client.execute({
        command: 'echo TEST_VAR=$TEST_VAR',
        envVars: { TEST_VAR: 'production' }
      });

      expect(result.output).toContain('TEST_VAR=production');
    });
  });

  describe('runPython', () => {
    it('should execute Python code successfully', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        stdout: 'Hello from Python!\n42',
        stderr: '',
        exit_code: 0,
        duration_ms: 67,
        request_id: 'python-test'
      }));

      const code = `
print("Hello from Python!")
result = 40 + 2
print(result)
`;

      const result = await client.runPython(code);

      expect(result.output).toContain('Hello from Python!');
      expect(result.output).toContain('42');
      expect(result.exitCode).toBe(0);
    });
  });

  describe('runJavaScript', () => {
    it('should execute JavaScript code successfully', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        stdout: 'Hello from JavaScript!\n42',
        stderr: '',
        exit_code: 0,
        duration_ms: 55,
        request_id: 'js-test'
      }));

      const code = 'console.log("Hello from JavaScript!"); console.log(42);';
      const result = await client.runJavaScript(code);

      expect(result.output).toContain('Hello from JavaScript!');
      expect(result.output).toContain('42');
      expect(result.exitCode).toBe(0);
    });
  });

  describe('runBash', () => {
    it('should execute Bash scripts successfully', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        stdout: 'Hello from Bash!\nCurrent date: 2024-01-15',
        stderr: '',
        exit_code: 0,
        duration_ms: 30,
        request_id: 'bash-test'
      }));

      const script = 'echo "Hello from Bash!"; echo "Current date: $(date +%Y-%m-%d)"';
      const result = await client.runBash(script);

      expect(result.output).toContain('Hello from Bash!');
      expect(result.exitCode).toBe(0);
    });
  });

  describe('forkExecution', () => {
    it('should fork execution from parent successfully', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        stdout: 'Forked execution result',
        stderr: '',
        exit_code: 0,
        duration_ms: 25,
        request_id: 'fork-test'
      }));

      const result = await client.forkExecution('parent-123', "echo 'Forked execution'");

      expect(result.output).toContain('Forked execution');
      expect(result.exitCode).toBe(0);
      expect(fetchMock).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/execute-advanced',
        expect.objectContaining({
          method: 'POST'
        })
      );
    });
  });

  describe('prewarm', () => {
    it('should prewarm containers successfully', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        message: 'Pre-warmed 3 containers',
        containers_created: 3
      }));

      await client.prewarm('python:3.11-slim', 3);

      expect(fetchMock).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/prewarm',
        expect.objectContaining({
          method: 'POST',
          body: expect.stringContaining('python:3.11-slim')
        })
      );
    });
  });

  describe('getMetrics', () => {
    it('should retrieve system metrics successfully', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        total_executions: 1547,
        avg_execution_time_ms: 87.5,
        cache_hit_rate: 0.73,
        active_containers: 15,
        active_instances: 5,
        memory_usage_mb: 2048,
        cpu_usage_percent: 45.3
      }));

      const metrics = await client.getMetrics();

      expect(metrics.totalExecutions).toBeGreaterThan(0);
      expect(metrics.avgExecutionTimeMs).toBeLessThan(200);
      expect(metrics.cacheHitRate).toBeGreaterThan(0);
      expect(fetchMock).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/metrics',
        expect.objectContaining({
          method: 'GET'
        })
      );
    });
  });

  describe('healthCheck', () => {
    it('should perform health check successfully', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        status: 'healthy',
        version: '1.0.0',
        uptime_seconds: 86400,
        components: {
          executor: 'healthy',
          docker: 'healthy',
          cache: 'healthy'
        }
      }));

      const health = await client.healthCheck();

      expect(health.status).toBe('healthy');
      expect(health.version).toBe('1.0.0');
      expect(fetchMock).toHaveBeenCalledWith(
        'http://localhost:8080/health',
        expect.objectContaining({
          method: 'GET'
        })
      );
    });
  });

  describe('error handling', () => {
    it('should handle server errors properly', async () => {
      fetchMock.mockRejectOnce(new Error('Network error'));

      await expect(client.execute({
        command: 'exit 1'
      })).rejects.toThrow('Network error');
    });

    it('should handle HTTP error responses', async () => {
      fetchMock.mockResponseOnce(JSON.stringify({
        error: 'Internal server error'
      }), { status: 500 });

      await expect(client.execute({
        command: 'exit 1'
      })).rejects.toThrow();
    });
  });

  describe('client configuration', () => {
    it('should create client with custom runtime', () => {
      const dockerClient = new FaaSClient('http://localhost:8080', Runtime.DOCKER);
      expect(dockerClient).toBeInstanceOf(FaaSClient);
    });

    it('should create client with different base URLs', () => {
      const prodClient = new FaaSClient('https://api.production.com');
      expect(prodClient).toBeInstanceOf(FaaSClient);
    });
  });

  describe('cache functionality', () => {
    it('should generate consistent cache keys', () => {
      const key1 = client.getCacheKey('test code');
      const key2 = client.getCacheKey('test code');
      const key3 = client.getCacheKey('different code');

      expect(key1).toBe(key2); // Same input should give same key
      expect(key1).not.toBe(key3); // Different input should give different key
      expect(key1).toHaveLength(32); // MD5 hash length
    });
  });

  describe('client metrics', () => {
    it('should track client-side metrics', () => {
      const metrics = client.getClientMetrics();

      expect(metrics).toHaveProperty('totalRequests');
      expect(metrics).toHaveProperty('cacheHitRate');
      expect(metrics).toHaveProperty('errorRate');
      expect(metrics.totalRequests).toBe(0); // New client should have 0 requests
    });
  });

  describe('enums', () => {
    it('should have correct Runtime enum values', () => {
      expect(Runtime.DOCKER).toBe('docker');
      expect(Runtime.FIRECRACKER).toBe('firecracker');
      expect(Runtime.AUTO).toBe('auto');
    });

    it('should have correct ExecutionMode enum values', () => {
      expect(ExecutionMode.NORMAL).toBeDefined();
      expect(ExecutionMode.CACHED).toBeDefined();
      expect(ExecutionMode.BRANCHED).toBeDefined();
    });
  });
});