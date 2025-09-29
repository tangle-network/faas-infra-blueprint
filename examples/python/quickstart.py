#!/usr/bin/env python3
"""
FaaS Platform Python Quick Start Examples

Demonstrates basic usage patterns and common operations.
"""

import asyncio
import sys
import os
# Fix the SDK path - use the correct location
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../../sdks/python')))

from faas_sdk import FaaSClient, Runtime, ExecutionMode


async def main():
    # Initialize client
    client = FaaSClient("http://localhost:8080")

    print("🚀 FaaS Platform Python Examples\n")

    # Example 1: Simple Python execution
    print("1. Running Python code:")
    result = await client.run_python('print("Hello from Python!")')
    print(f"   Output: {result.output}")
    print(f"   Duration: {result.duration_ms}ms")
    print(f"   Cache hit: {result.cache_hit}\n")

    # Example 2: JavaScript execution
    print("2. Running JavaScript code:")
    result = await client.run_javascript('console.log("Hello from Node.js!")')
    print(f"   Output: {result.output}")
    print(f"   Duration: {result.duration_ms}ms\n")

    # Example 3: Bash script execution
    print("3. Running Bash script:")
    result = await client.run_bash('''
        echo "System info:"
        uname -a
        echo "Memory:"
        free -h | head -2
    ''')
    print(f"   Output:\n{result.output}\n")

    # Example 4: Using Docker runtime explicitly
    print("4. Using Docker runtime:")
    result = await client.execute(
        command='echo "Running in Docker container"',
        runtime=Runtime.DOCKER
    )
    print(f"   Output: {result.output}")
    print(f"   Runtime used: {result.runtime_used}\n")

    # Example 5: Using environment variables
    print("5. With environment variables:")
    result = await client.execute(
        command='python -c "import os; print(f\'API_KEY={os.environ.get(\'API_KEY\', \'not set\')}\')"',
        image="python:3.11-slim",
        env_vars={"API_KEY": "secret123"}
    )
    print(f"   Output: {result.output}\n")

    # Example 6: Caching demonstration
    print("6. Caching demonstration:")

    # First execution (cold)
    result1 = await client.run_python('import time; time.sleep(1); print("Computed result")')
    print(f"   First run: {result1.duration_ms}ms (cache hit: {result1.cache_hit})")

    # Second execution (should be cached)
    result2 = await client.run_python('import time; time.sleep(1); print("Computed result")')
    print(f"   Second run: {result2.duration_ms}ms (cache hit: {result2.cache_hit})")

    if result2.duration_ms < result1.duration_ms / 10:
        print("   ✅ Caching working! Second run was much faster\n")

    # Example 7: Pre-warming containers
    print("7. Pre-warming containers:")
    await client.prewarm("python:3.11-slim", count=3)
    print("   Pre-warmed 3 Python containers for instant execution\n")

    # Example 8: Error handling
    print("8. Error handling:")
    try:
        result = await client.run_python('import sys; sys.exit(1)')
        if result.error:
            print(f"   Error caught: {result.error}\n")
    except Exception as e:
        print(f"   Exception: {e}\n")

    # Example 9: Getting metrics
    print("9. Platform metrics:")

    # Server metrics
    server_metrics = await client.get_metrics()
    print(f"   Server metrics: {server_metrics}")

    # Client metrics
    client_metrics = client.get_client_metrics()
    print(f"   Client metrics:")
    print(f"     Total requests: {client_metrics.total_requests}")
    print(f"     Cache hit rate: {client_metrics.cache_hit_rate:.2%}")
    print(f"     Avg latency: {client_metrics.average_latency_ms:.2f}ms\n")

    # Example 10: Health check
    print("10. Platform health:")
    health = await client.health_check()
    print(f"    Status: {health['status']}")
    print(f"    Components: {health['components']}")

    await client.session.close()


if __name__ == "__main__":
    asyncio.run(main())