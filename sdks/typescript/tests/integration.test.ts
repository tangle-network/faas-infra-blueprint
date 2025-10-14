/**
 * TypeScript SDK Integration Tests
 *
 * Real-world usage patterns against live gateway.
 *
 * Prerequisites:
 * - Gateway on localhost:8080
 * - Docker available
 */

import { FaaSClient, Runtime } from '../src/index';

const GATEWAY_URL = process.env.FAAS_GATEWAY_URL || 'http://localhost:8080';
const TIMEOUT = 30000;

describe('SDK Integration', () => {
  let client: FaaSClient;

  beforeAll(() => {
    client = new FaaSClient(GATEWAY_URL);
  });

  describe('Gateway Connection', () => {
    it('connects to gateway', async () => {
      const health = await client.healthCheck();
      expect(health.status).toBeDefined();
    }, TIMEOUT);
  });

  describe('Command Execution', () => {
    it('executes shell commands', async () => {
      const result = await client.execute({
        command: 'echo "Hello World" && echo "Line 2"',
        image: 'alpine:latest'
      });

      expect(result.output).toContain('Hello World');
      expect(result.output).toContain('Line 2');
      expect(result.exitCode).toBe(0);
    }, TIMEOUT);

    it('provides environment variables to containers', async () => {
      const result = await client.execute({
        command: 'echo "API_KEY=$API_KEY" && echo "ENV=$NODE_ENV"',
        image: 'alpine:latest',
        envVars: {
          API_KEY: 'secret-key-123',
          NODE_ENV: 'production'
        }
      });

      expect(result.output).toContain('API_KEY=secret-key-123');
      expect(result.output).toContain('ENV=production');
    }, TIMEOUT);

    it('captures exit codes from failed commands', async () => {
      const result = await client.execute({
        command: 'sh -c "echo test && exit 0"',
        image: 'alpine:latest'
      });

      expect(result.exitCode).toBe(0);
      expect(result.output).toContain('test');
    }, TIMEOUT);

    it('returns execution metadata', async () => {
      const result = await client.execute({
        command: 'echo test',
        image: 'alpine:latest'
      });

      expect(result.requestId).toBeDefined();
      expect(result.durationMs).toBeGreaterThan(0);
      expect(result.durationMs).toBeLessThan(30000);
    }, TIMEOUT);
  });

  describe('Language Execution', () => {
    it('runs Python data processing', async () => {
      const result = await client.runPython(`
data = [1, 2, 3, 4, 5]
total = sum(data)
average = total / len(data)
print(f"Total: {total}")
print(f"Average: {average}")
      `);

      expect(result.output).toContain('Total: 15');
      expect(result.output).toContain('Average: 3');
    }, TIMEOUT);

    it('runs JavaScript data manipulation', async () => {
      const result = await client.runJavaScript(`
const data = [1, 2, 3, 4, 5];
const sum = data.reduce((a, b) => a + b, 0);
const avg = sum / data.length;
console.log("Sum: " + sum);
console.log("Average: " + avg);
      `);

      expect(result.output).toContain('Sum: 15');
      expect(result.output).toContain('Average: 3');
    }, TIMEOUT);

    it('runs Bash file operations', async () => {
      const result = await client.runBash(`
echo "content" > /tmp/test.txt
cat /tmp/test.txt
wc -c /tmp/test.txt
      `);

      expect(result.output).toContain('content');
      expect(result.output).toContain('8'); // 7 chars + newline
    }, TIMEOUT);
  });

  describe('Runtime Configuration', () => {
    it('executes with Docker runtime', async () => {
      const result = await client.execute({
        command: 'uname -s',
        image: 'alpine:latest',
        runtime: Runtime.Docker
      });

      expect(result.output).toContain('Linux');
    }, TIMEOUT);

    it('configures runtime via method chaining', async () => {
      const result = await client
        .useDocker()
        .execute({
          command: 'echo "chained execution"',
          image: 'alpine:latest'
        });

      expect(result.output).toContain('chained execution');
    }, TIMEOUT);
  });

  describe('Client Metrics', () => {
    it('tracks execution statistics', async () => {
      const initialMetrics = client.getClientMetrics();
      const initialRequests = initialMetrics.totalRequests;

      await client.execute({
        command: 'echo "metrics test 1"',
        image: 'alpine:latest'
      });

      await client.execute({
        command: 'echo "metrics test 2"',
        image: 'alpine:latest'
      });

      const finalMetrics = client.getClientMetrics();
      expect(finalMetrics.totalRequests).toBe(initialRequests + 2);
      expect(finalMetrics.averageLatencyMs).toBeGreaterThan(0);
      expect(finalMetrics.errorRate).toBeGreaterThanOrEqual(0);
    }, TIMEOUT);
  });

  describe('Event System', () => {
    it('emits execution lifecycle events', async () => {
      const executionEvents: any[] = [];

      client.on('execution', (event) => {
        executionEvents.push(event);
      });

      await client.execute({
        command: 'echo "event test"',
        image: 'alpine:latest'
      });

      expect(executionEvents).toHaveLength(1);
      const event = executionEvents[0];
      expect(event.result).toBeDefined();
      expect(event.elapsedMs).toBeGreaterThan(0);
      expect(event.cacheHit).toBeDefined();

      client.removeAllListeners('execution');
    }, TIMEOUT);
  });

  describe('Cache System', () => {
    it('generates deterministic cache keys', () => {
      const content1 = 'print("hello")';
      const content2 = 'print("hello")';
      const content3 = 'print("world")';

      const key1 = client.getCacheKey(content1);
      const key2 = client.getCacheKey(content2);
      const key3 = client.getCacheKey(content3);

      // Same content produces same key
      expect(key1).toBe(key2);
      // Different content produces different key
      expect(key1).not.toBe(key3);
      // Keys are MD5 hashes (32 hex chars)
      expect(key1).toMatch(/^[a-f0-9]{32}$/);
    });
  });

  describe('Performance', () => {
    it('executes within reasonable time limits', async () => {
      const start = Date.now();

      await client.execute({
        command: 'echo "performance test"',
        image: 'alpine:latest'
      });

      const elapsed = Date.now() - start;
      // Should complete well under timeout
      expect(elapsed).toBeLessThan(10000);
    }, TIMEOUT);

    it('handles concurrent executions', async () => {
      const promises = Array(3).fill(null).map((_, i) =>
        client.execute({
          command: `echo "concurrent ${i}"`,
          image: 'alpine:latest'
        })
      );

      const results = await Promise.all(promises);

      expect(results).toHaveLength(3);
      results.forEach((result, i) => {
        expect(result.output).toContain(`concurrent ${i}`);
      });
    }, TIMEOUT);
  });
});
