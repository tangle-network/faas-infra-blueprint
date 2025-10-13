/**
 * Integration tests for TypeScript SDK
 *
 * These tests run against a real faas-gateway-server instance.
 *
 * Prerequisites:
 * - faas-gateway-server must be running on localhost:8080
 * - Docker must be available for container execution
 *
 * To run:
 * 1. Start the gateway: cargo run --package faas-gateway-server --release
 * 2. Run tests: npm run test:integration
 */

import { FaaSClient, Runtime } from '../src/index';

const GATEWAY_URL = process.env.FAAS_GATEWAY_URL || 'http://localhost:8080';
const TEST_TIMEOUT = 30000; // 30 seconds for integration tests

describe('FaaS SDK Integration Tests', () => {
  let client: FaaSClient;

  beforeAll(() => {
    client = new FaaSClient(GATEWAY_URL);
  });

  describe('Health Check', () => {
    it('should verify gateway is running', async () => {
      const health = await client.healthCheck();
      expect(health.status).toBeDefined();
    }, TEST_TIMEOUT);
  });

  describe('Basic Execution', () => {
    it('should execute simple echo command', async () => {
      const result = await client.execute({
        command: 'echo "Hello from Docker"',
        image: 'alpine:latest'
      });

      expect(result.output).toContain('Hello from Docker');
      expect(result.requestId).toBeDefined();
      expect(result.durationMs).toBeGreaterThan(0);
    }, TEST_TIMEOUT);

    it('should execute command with environment variables', async () => {
      const result = await client.execute({
        command: 'echo $MY_VAR',
        image: 'alpine:latest',
        envVars: { MY_VAR: 'test-value' }
      });

      expect(result.output).toContain('test-value');
    }, TEST_TIMEOUT);
  });

  describe('Language Runtimes', () => {
    it('should run Python code', async () => {
      const code = 'print("Python works"); print(2 + 2)';
      const result = await client.runPython(code);

      expect(result.output).toContain('Python works');
      expect(result.output).toContain('4');
    }, TEST_TIMEOUT);

    it('should run JavaScript code', async () => {
      const code = 'console.log("JavaScript works"); console.log(2 + 2)';
      const result = await client.runJavaScript(code);

      expect(result.output).toContain('JavaScript works');
      expect(result.output).toContain('4');
    }, TEST_TIMEOUT);

    it('should run Bash script', async () => {
      const script = 'echo "Bash works"; expr 2 + 2';
      const result = await client.runBash(script);

      expect(result.output).toContain('Bash works');
      expect(result.output).toContain('4');
    }, TEST_TIMEOUT);
  });

  describe('Runtime Selection', () => {
    it('should execute with Docker runtime', async () => {
      const result = await client.execute({
        command: 'echo "Docker runtime"',
        image: 'alpine:latest',
        runtime: Runtime.Docker
      });

      expect(result.output).toContain('Docker runtime');
    }, TEST_TIMEOUT);

    it('should execute with method chaining', async () => {
      const result = await client
        .useDocker()
        .execute({
          command: 'echo "Chained execution"',
          image: 'alpine:latest'
        });

      expect(result.output).toContain('Chained execution');
    }, TEST_TIMEOUT);
  });

  describe('Performance', () => {
    it('should complete execution in reasonable time', async () => {
      const start = Date.now();

      await client.execute({
        command: 'echo "Performance test"',
        image: 'alpine:latest'
      });

      const elapsed = Date.now() - start;
      expect(elapsed).toBeLessThan(10000); // Should complete within 10 seconds
    }, TEST_TIMEOUT);
  });

  describe('Error Handling', () => {
    it('should handle command that exits with error', async () => {
      const result = await client.execute({
        command: 'sh -c "echo error output && exit 1"',
        image: 'alpine:latest'
      });

      // The execution should complete, but may have error output
      expect(result.requestId).toBeDefined();
    }, TEST_TIMEOUT);

    it.skip('should handle timeout gracefully', async () => {
      await expect(
        client.execute({
          command: 'sleep 60',
          image: 'alpine:latest',
          timeoutMs: 1000
        })
      ).rejects.toThrow();
    }, TEST_TIMEOUT);
  });

  describe('Event Emitter', () => {
    it('should emit execution events', async () => {
      const events: any[] = [];

      client.on('execution', (event) => {
        events.push(event);
      });

      await client.execute({
        command: 'echo "Event test"',
        image: 'alpine:latest'
      });

      expect(events.length).toBeGreaterThan(0);
      expect(events[0]).toHaveProperty('result');
      expect(events[0]).toHaveProperty('elapsedMs');
    }, TEST_TIMEOUT);
  });

  describe('Client Metrics', () => {
    it('should track client-side metrics', async () => {
      const metricsBefore = client.getClientMetrics();
      const requestsBefore = metricsBefore.totalRequests;

      await client.execute({
        command: 'echo "Metrics test"',
        image: 'alpine:latest'
      });

      const metricsAfter = client.getClientMetrics();
      expect(metricsAfter.totalRequests).toBe(requestsBefore + 1);
      expect(metricsAfter.totalLatencyMs).toBeGreaterThan(0);
    }, TEST_TIMEOUT);
  });
});
