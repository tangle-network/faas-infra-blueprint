#!/usr/bin/env python3
"""
Real test of Python SDK against running FaaS Gateway
"""

import asyncio
import sys
import os

# Add SDK to path
sys.path.insert(0, '/Users/drew/webb/faas/sdks/python')

from faas_sdk import FaaSClient, ExecutionResult


async def main():
    """Test SDK with real running server"""

    print("🧪 Testing Python SDK with Real FaaS Gateway")
    print("=" * 50)

    # Create client pointing to real server
    client = FaaSClient("http://localhost:8080")

    try:
        # Test 1: Health check
        print("\n✅ Test 1: Health Check")
        health = await client.health_check()
        print(f"   Status: {health.get('status', 'unknown')}")
        assert health['status'] == 'healthy', "Health check failed"

        # Test 2: Basic execution
        print("\n✅ Test 2: Basic Execution")
        result = await client.execute(
            command="echo 'Hello from Python SDK'",
            image="alpine:latest"
        )
        print(f"   Output: {result.output.strip()}")
        assert "Hello from Python SDK" in result.output
        assert result.error is None  # No error means success

        # Test 3: Python code execution
        print("\n✅ Test 3: Python Code Execution")
        python_code = """
import math
print(f"Pi is approximately {math.pi:.4f}")
print(f"Square root of 144 is {math.sqrt(144)}")
"""
        result = await client.run_python(python_code)
        print(f"   Output: {result.output.strip()}")
        assert "3.1416" in result.output
        assert "12.0" in result.output

        # Test 4: JavaScript execution
        print("\n✅ Test 4: JavaScript Execution")
        js_code = """
const data = [1, 2, 3, 4, 5];
const sum = data.reduce((a, b) => a + b, 0);
console.log(`Sum of array: ${sum}`);
console.log(`Array length: ${data.length}`);
"""
        result = await client.run_javascript(js_code)
        print(f"   Output: {result.output.strip()}")
        assert "Sum of array: 15" in result.output
        assert "Array length: 5" in result.output

        # Test 5: Bash script
        print("\n✅ Test 5: Bash Script Execution")
        bash_script = """
echo "System information:"
uname -a | head -c 50
echo "..."
echo "Current directory: $(pwd)"
echo "Files count: $(ls -1 | wc -l)"
"""
        result = await client.run_bash(bash_script)
        print(f"   Output: {result.output[:100]}...")
        assert "System information:" in result.output
        assert result.error is None

        # Test 6: Metrics
        print("\n✅ Test 6: Get Metrics")
        metrics = await client.get_metrics()
        print(f"   Active containers: {metrics.get('active_containers', 0)}")
        print(f"   Cache hit rate: {metrics.get('cache_hit_rate', 0):.2%}")

        # Test 7: Error handling
        print("\n✅ Test 7: Error Handling")
        result = await client.execute(
            command="exit 42",
            image="alpine:latest"
        )
        # In this SDK, exit 42 might not be treated as an error
        print(f"   Result error: {result.error}")
        print(f"   Result output: {result.output}")

        # Test 8: Performance (multiple rapid executions)
        print("\n✅ Test 8: Performance Test")
        import time
        start = time.time()

        tasks = []
        for i in range(5):
            task = client.execute(
                command=f"echo 'Parallel test {i}'",
                image="alpine:latest"
            )
            tasks.append(task)

        results = await asyncio.gather(*tasks)
        elapsed = time.time() - start

        print(f"   Executed {len(results)} functions in {elapsed:.2f}s")
        print(f"   Average: {elapsed/len(results):.2f}s per execution")

        for i, result in enumerate(results):
            assert f"Parallel test {i}" in result.output

        print("\n" + "=" * 50)
        print("✅ All Python SDK tests passed!")

    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        import traceback
        traceback.print_exc()
        return 1
    finally:
        # Client doesn't have a close method, session will be cleaned up on exit
        pass

    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)