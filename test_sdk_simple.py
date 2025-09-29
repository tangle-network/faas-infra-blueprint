#!/usr/bin/env python3
"""
Simple SDK test to verify basic functionality
"""

import asyncio
import sys
sys.path.insert(0, '/Users/drew/webb/faas/sdks/python')

from faas_sdk import FaaSClient


async def main():
    print("🧪 Simple FaaS SDK Test")
    print("=" * 40)

    client = FaaSClient("http://localhost:8080")

    # Test 1: Basic echo
    print("\n1. Basic echo test...")
    result = await client.execute(
        command="echo Testing FaaS Platform",
        image="alpine:latest"
    )
    print(f"   Result: {result.output.strip()}")
    assert "Testing FaaS Platform" in result.output

    # Test 2: Simple math with Python
    print("\n2. Python math test...")
    result = await client.execute(
        command="python3 -c 'print(2 + 2)'",
        image="python:3.11-slim"
    )
    print(f"   Result: {result.output.strip()}")
    assert "4" in result.output

    # Test 3: Node.js test
    print("\n3. Node.js test...")
    result = await client.execute(
        command="node -e 'console.log(Math.pow(2, 3))'",
        image="node:18-alpine"
    )
    print(f"   Result: {result.output.strip()}")
    assert "8" in result.output

    # Test 4: Check metrics
    print("\n4. Metrics test...")
    metrics = await client.get_metrics()
    print(f"   Active containers: {metrics.get('active_containers', 0)}")

    # Test 5: Health check
    print("\n5. Health check...")
    health = await client.health_check()
    print(f"   Status: {health['status']}")
    assert health['status'] == 'healthy'

    print("\n" + "=" * 40)
    print("✅ All tests passed!")


if __name__ == "__main__":
    asyncio.run(main())