#!/usr/bin/env python3
"""
Complete showcase of all FaaS Platform SDK top-level API methods.

This example demonstrates every documented API method:
- execute: Generic command execution
- run_python: Python code execution
- run_javascript: JavaScript code execution
- fork_execution: A/B testing and parallel execution
- create_snapshot: Container state snapshots
- prewarm: Container prewarming
- get_metrics: Platform metrics
- health: Health check
"""

import asyncio
import json
from datetime import datetime
from faas_sdk import FaaSClient, Runtime, ForkStrategy, ExecutionMode

async def main():
    """Demonstrate all SDK capabilities."""

    # Initialize client
    client = FaaSClient("http://localhost:8080")

    print("🚀 FaaS Platform - Complete API Showcase\n")
    print("=" * 60)

    # ========================================
    # 1. EXECUTE - Generic command execution
    # ========================================
    print("\n1️⃣  EXECUTE - Generic Command Execution")
    print("-" * 40)

    # Basic execution
    result = await client.execute(
        command="echo 'Hello from FaaS Platform!'",
        image="alpine:latest"
    )
    print(f"✓ Basic: {result.stdout.strip()}")
    print(f"  Duration: {result.duration_ms}ms")

    # With environment variables
    result = await client.execute(
        command="sh -c 'echo $USER is using $PLATFORM'",
        image="alpine:latest",
        env_vars=[
            ("USER", "Developer"),
            ("PLATFORM", "FaaS")
        ]
    )
    print(f"✓ With env: {result.stdout.strip()}")

    # With runtime selection
    result = await client.execute(
        command="echo 'Running on Firecracker'",
        image="alpine:latest",
        runtime=Runtime.FIRECRACKER
    )
    print(f"✓ Firecracker: {result.stdout.strip()}")

    # With caching
    cache_key = "expensive-computation-123"
    result1 = await client.execute(
        command="sleep 1 && echo 'Computed result'",
        image="alpine:latest",
        cache_key=cache_key
    )
    print(f"✓ First run (cached={result1.cached}): {result1.duration_ms}ms")

    result2 = await client.execute(
        command="sleep 1 && echo 'Computed result'",
        image="alpine:latest",
        cache_key=cache_key
    )
    print(f"✓ Second run (cached={result2.cached}): {result2.duration_ms}ms")

    # ========================================
    # 2. RUN_PYTHON - Python code execution
    # ========================================
    print("\n2️⃣  RUN_PYTHON - Python Code Execution")
    print("-" * 40)

    # Simple Python
    python_code = """
import sys
print(f"Python {sys.version.split()[0]}")
result = sum([1, 2, 3, 4, 5])
print(f"Sum: {result}")
"""
    result = await client.run_python(python_code)
    print(f"✓ Basic Python:\n{result.stdout}")

    # Python with data science libraries
    ml_code = """
import json
# Simulate ML prediction
data = {"features": [1.5, 2.3, 0.8]}
prediction = sum(data["features"]) * 0.7  # Mock prediction
print(json.dumps({"prediction": prediction, "confidence": 0.92}))
"""
    result = await client.run_python(ml_code, image="python:3.11-slim")
    print(f"✓ ML Prediction: {result.stdout.strip()}")

    # ========================================
    # 3. RUN_JAVASCRIPT - JavaScript execution
    # ========================================
    print("\n3️⃣  RUN_JAVASCRIPT - JavaScript Code Execution")
    print("-" * 40)

    # Simple JavaScript
    js_code = """
console.log('Node.js', process.version);
const numbers = [1, 2, 3, 4, 5];
const sum = numbers.reduce((a, b) => a + b, 0);
console.log('Sum:', sum);
"""
    result = await client.run_javascript(js_code)
    print(f"✓ Basic JavaScript:\n{result.stdout}")

    # JavaScript with async/await
    async_js = """
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

async function processData() {
    console.log('Processing...');
    await delay(100);
    return { status: 'complete', items: 42 };
}

processData().then(result => {
    console.log('Result:', JSON.stringify(result));
});
"""
    result = await client.run_javascript(async_js, image="node:20-alpine")
    print(f"✓ Async JavaScript: {result.stdout.strip()}")

    # ========================================
    # 4. FORK_EXECUTION - A/B Testing
    # ========================================
    print("\n4️⃣  FORK_EXECUTION - A/B Testing & Parallel Execution")
    print("-" * 40)

    # A/B test different algorithms
    branches = [
        {
            "id": "algorithm-v1",
            "command": "echo '{\"version\": \"v1\", \"result\": 42}'",
            "weight": 0.5
        },
        {
            "id": "algorithm-v2",
            "command": "echo '{\"version\": \"v2\", \"result\": 47}'",
            "weight": 0.5
        }
    ]

    fork_result = await client.fork_execution(
        branches=branches,
        image="alpine:latest",
        strategy=ForkStrategy.PARALLEL
    )

    print(f"✓ A/B Test Results:")
    for result in fork_result.results:
        print(f"  - {result['branch_id']}: {result['stdout'].strip()} "
              f"({result['duration_ms']}ms)")
    print(f"✓ Selected: {fork_result.selected_branch} "
          f"(reason: {fork_result.selection_reason})")

    # Performance comparison
    perf_branches = [
        {
            "id": "optimized",
            "command": "echo 'Fast algorithm'",
            "weight": 0.5
        },
        {
            "id": "standard",
            "command": "sleep 0.1 && echo 'Standard algorithm'",
            "weight": 0.5
        }
    ]

    perf_result = await client.fork_execution(
        branches=perf_branches,
        image="alpine:latest",
        strategy=ForkStrategy.FASTEST
    )

    print(f"✓ Performance Test: {perf_result.selected_branch} was fastest")

    # ========================================
    # 5. CREATE_SNAPSHOT - Container snapshots
    # ========================================
    print("\n5️⃣  CREATE_SNAPSHOT - Container State Snapshots")
    print("-" * 40)

    # First create a container with state
    setup_result = await client.execute(
        command="echo 'Initial state' > /tmp/state.txt && echo 'State created'",
        image="alpine:latest"
    )

    # Create snapshot (if supported)
    try:
        snapshot = await client.create_snapshot(
            name="demo-snapshot",
            container_id=setup_result.request_id,
            description="Showcase example snapshot"
        )
        print(f"✓ Created snapshot: {snapshot.snapshot_id}")
        print(f"  Size: {snapshot.size_bytes:,} bytes")
        print(f"  Compression: {snapshot.compression_ratio:.2f}x")
    except Exception as e:
        print(f"ℹ️  Snapshots not available in current environment: {e}")

    # List snapshots
    try:
        snapshots = await client.list_snapshots()
        print(f"✓ Total snapshots: {len(snapshots)}")
    except:
        pass

    # ========================================
    # 6. PREWARM - Container prewarming
    # ========================================
    print("\n6️⃣  PREWARM - Container Prewarming")
    print("-" * 40)

    # Prewarm Docker containers
    prewarm_result = await client.prewarm(
        image="alpine:latest",
        count=3,
        runtime=Runtime.DOCKER
    )
    print(f"✓ Docker prewarming:")
    print(f"  - Containers warmed: {prewarm_result['containers_warmed']}")
    print(f"  - Average warmup: {prewarm_result['average_warmup_ms']}ms")

    # Prewarm Firecracker microVMs
    try:
        fc_prewarm = await client.prewarm(
            image="alpine:latest",
            count=2,
            runtime=Runtime.FIRECRACKER,
            memory_mb=256,
            cpu_cores=1
        )
        print(f"✓ Firecracker prewarming:")
        print(f"  - MicroVMs warmed: {fc_prewarm['containers_warmed']}")
        print(f"  - Average warmup: {fc_prewarm['average_warmup_ms']}ms")
    except Exception as e:
        print(f"ℹ️  Firecracker prewarming not available: {e}")

    # ========================================
    # 7. GET_METRICS - Platform metrics
    # ========================================
    print("\n7️⃣  GET_METRICS - Platform Performance Metrics")
    print("-" * 40)

    metrics = await client.get_metrics()

    print(f"✓ Platform Metrics:")
    print(f"  - Total executions: {metrics.get('total_executions', 0):,}")
    print(f"  - Avg execution time: {metrics.get('avg_execution_time_ms', 0):.2f}ms")
    print(f"  - Cache hit rate: {metrics.get('cache_hit_rate', 0):.1%}")
    print(f"  - Active containers: {metrics.get('active_containers', 0)}")
    print(f"  - Memory usage: {metrics.get('memory_usage_mb', 0):,}MB")
    print(f"  - CPU usage: {metrics.get('cpu_usage_percent', 0):.1f}%")

    # Performance percentiles
    if 'p50_latency_ms' in metrics:
        print(f"\n✓ Latency Percentiles:")
        print(f"  - P50: {metrics['p50_latency_ms']}ms")
        print(f"  - P95: {metrics.get('p95_latency_ms', 0)}ms")
        print(f"  - P99: {metrics.get('p99_latency_ms', 0)}ms")

    # Warm start performance
    if 'warm_start_ratio' in metrics:
        print(f"\n✓ Performance:")
        print(f"  - Warm start ratio: {metrics['warm_start_ratio']:.1%}")
        print(f"  - Cold starts (1hr): {metrics.get('cold_starts_last_hour', 0)}")

    # ========================================
    # 8. HEALTH - Health check
    # ========================================
    print("\n8️⃣  HEALTH - Platform Health Check")
    print("-" * 40)

    health = await client.health()

    print(f"✓ Health Status: {health['status'].upper()}")
    print(f"  - Uptime: {health.get('uptime_seconds', 0) // 3600}h "
          f"{(health.get('uptime_seconds', 0) % 3600) // 60}m")

    if 'version' in health:
        print(f"  - Version: {health['version']}")

    if 'components' in health:
        print(f"\n✓ Component Health:")
        for component, status in health['components'].items():
            emoji = "✅" if status == "healthy" else "⚠️"
            print(f"  {emoji} {component}: {status}")

    if 'issues' in health and health['issues']:
        print(f"\n⚠️  Current Issues:")
        for issue in health['issues']:
            print(f"  - {issue}")

    # ========================================
    # PERFORMANCE VERIFICATION
    # ========================================
    print("\n🏁 PERFORMANCE VERIFICATION")
    print("=" * 60)

    # Verify warm starts < 50ms
    print("\n✓ Warm Start Performance (<50ms target):")
    for i in range(3):
        result = await client.execute(
            command="echo 'warm test'",
            image="alpine:latest",
            cache_key=f"warm-benchmark-{i}"
        )
        status = "✅" if result.duration_ms < 50 else "⚠️"
        print(f"  {status} Attempt {i+1}: {result.duration_ms}ms")

    # Verify branching < 250ms
    print("\n✓ Branching Performance (<250ms target):")
    start_time = datetime.now()
    branch_result = await client.fork_execution(
        branches=[
            {"id": "branch1", "command": "echo '1'"},
            {"id": "branch2", "command": "echo '2'"}
        ],
        image="alpine:latest",
        strategy=ForkStrategy.PARALLEL
    )
    branch_duration = (datetime.now() - start_time).total_seconds() * 1000
    status = "✅" if branch_duration < 250 else "⚠️"
    print(f"  {status} Branching time: {branch_duration:.0f}ms")

    print("\n" + "=" * 60)
    print("✨ All API methods demonstrated successfully!")
    print("=" * 60)

if __name__ == "__main__":
    asyncio.run(main())