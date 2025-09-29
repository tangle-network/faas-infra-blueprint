#!/usr/bin/env ts-node
/**
 * FaaS Platform TypeScript Quick Start Examples
 *
 * Demonstrates basic usage patterns and common operations.
 */

import { FaaSClient, Runtime, ExecutionMode } from '../../sdks/typescript/src';

async function main() {
  // Initialize client
  const client = new FaaSClient('http://localhost:8080');

  console.log('🚀 FaaS Platform TypeScript Examples\n');

  // Example 1: Simple Python execution
  console.log('1. Running Python code:');
  let result = await client.runPython('print("Hello from Python!")');
  console.log(`   Output: ${result.output}`);
  console.log(`   Duration: ${result.durationMs}ms`);
  console.log(`   Cache hit: ${result.cacheHit}\n`);

  // Example 2: JavaScript execution
  console.log('2. Running JavaScript code:');
  result = await client.runJavaScript('console.log("Hello from Node.js!")');
  console.log(`   Output: ${result.output}`);
  console.log(`   Duration: ${result.durationMs}ms\n`);

  // Example 3: TypeScript execution
  console.log('3. Running TypeScript code:');
  result = await client.runTypeScript(`
    interface Person {
      name: string;
      age: number;
    }
    const person: Person = { name: "Alice", age: 30 };
    console.log(\`Hello, \${person.name}!\`);
  `);
  console.log(`   Output: ${result.output}\n`);

  // Example 4: Bash script execution
  console.log('4. Running Bash script:');
  result = await client.runBash(`
    echo "System info:"
    uname -a
    echo "Node version:"
    node --version 2>/dev/null || echo "Node not available"
  `);
  console.log(`   Output:\n${result.output}\n`);

  // Example 5: Using Docker runtime explicitly
  console.log('5. Using Docker runtime:');
  result = await client.execute({
    command: 'echo "Running in Docker container"',
    runtime: Runtime.Docker
  });
  console.log(`   Output: ${result.output}`);
  console.log(`   Runtime used: ${result.runtimeUsed}\n`);

  // Example 6: Using environment variables
  console.log('6. With environment variables:');
  result = await client.execute({
    command: 'node -e "console.log(`API_KEY=${process.env.API_KEY}`)"',
    image: 'node:20-slim',
    envVars: { API_KEY: 'secret123' }
  });
  console.log(`   Output: ${result.output}\n`);

  // Example 7: Caching demonstration
  console.log('7. Caching demonstration:');

  // First execution (cold)
  const start1 = Date.now();
  const result1 = await client.runJavaScript(`
    const start = Date.now();
    while(Date.now() - start < 1000) {} // Simulate work
    console.log("Computed result");
  `);
  console.log(`   First run: ${result1.durationMs}ms (cache hit: ${result1.cacheHit})`);

  // Second execution (should be cached)
  const result2 = await client.runJavaScript(`
    const start = Date.now();
    while(Date.now() - start < 1000) {} // Simulate work
    console.log("Computed result");
  `);
  console.log(`   Second run: ${result2.durationMs}ms (cache hit: ${result2.cacheHit})`);

  if (result2.durationMs < result1.durationMs / 10) {
    console.log('   ✅ Caching working! Second run was much faster\n');
  }

  // Example 8: Pre-warming containers
  console.log('8. Pre-warming containers:');
  await client.prewarm('node:20-slim', 3);
  console.log('   Pre-warmed 3 Node.js containers for instant execution\n');

  // Example 9: Error handling
  console.log('9. Error handling:');
  try {
    result = await client.runJavaScript('throw new Error("Intentional error")');
    if (result.error) {
      console.log(`   Error caught: ${result.error}\n`);
    }
  } catch (error) {
    console.log(`   Exception: ${error}\n`);
  }

  // Example 10: Event listeners
  console.log('10. Event listeners:');
  client.on('execution', (event) => {
    console.log(`   Execution event: ${event.elapsedMs}ms, cached: ${event.cacheHit}`);
  });

  client.on('retry', (event) => {
    console.log(`   Retry attempt ${event.attempt}: ${event.error}`);
  });

  await client.runPython('print("Testing events")');

  // Example 11: Getting metrics
  console.log('\n11. Platform metrics:');

  // Server metrics
  const serverMetrics = await client.getMetrics();
  console.log(`   Server metrics:`, serverMetrics);

  // Client metrics
  const clientMetrics = client.getClientMetrics();
  console.log(`   Client metrics:`);
  console.log(`     Total requests: ${clientMetrics.totalRequests}`);
  console.log(`     Cache hit rate: ${(clientMetrics.cacheHitRate * 100).toFixed(2)}%`);
  console.log(`     Avg latency: ${clientMetrics.averageLatencyMs.toFixed(2)}ms\n`);

  // Example 12: Health check
  console.log('12. Platform health:');
  const health = await client.healthCheck();
  console.log(`    Status: ${health.status}`);
  console.log(`    Components:`, health.components);

  // Example 13: Method chaining
  console.log('\n13. Method chaining:');
  const dockerClient = new FaaSClient('http://localhost:8080')
    .useDocker()
    .setCaching(true);

  result = await dockerClient.runPython('print("Using Docker with caching")');
  console.log(`   Output: ${result.output}`);

  const firecrackerClient = new FaaSClient('http://localhost:8080')
    .useFirecracker()
    .setCaching(false);

  try {
    result = await firecrackerClient.runPython('print("Using Firecracker VMs")');
    console.log(`   Output: ${result.output}`);
  } catch (error) {
    console.log('   Firecracker not available on this system');
  }
}

// Run the examples
main()
  .then(() => {
    console.log('\n✅ All examples completed successfully!');
    process.exit(0);
  })
  .catch((error) => {
    console.error('\n❌ Error running examples:', error);
    process.exit(1);
  });