#!/usr/bin/env python3
"""
FaaS Platform Advanced Python Examples

Demonstrates advanced features like forking, snapshots, and streaming.
"""

import asyncio
import sys
import os
# Fix the SDK path - use the correct location
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../../sdks/python')))

from faas_sdk import FaaSClient, Runtime, ExecutionMode, ForkStrategy


async def example_forking():
    """Demonstrate execution forking for A/B testing"""
    client = FaaSClient("http://localhost:8080")

    print("🔀 Execution Forking Example\n")

    # Create parent execution
    print("1. Creating parent execution:")
    parent = await client.execute(
        command='echo "Parent state: initialized" > /tmp/state.txt && cat /tmp/state.txt',
        image="alpine:latest"
    )
    print(f"   Parent ID: {parent.request_id}")
    print(f"   Output: {parent.output}\n")

    # Fork from parent
    print("2. Forking execution A:")
    fork_a = await client.fork_execution(
        parent_id=parent.request_id,
        command='echo "Fork A modification" >> /tmp/state.txt && cat /tmp/state.txt'
    )
    print(f"   Fork A output:\n{fork_a.output}\n")

    print("3. Forking execution B:")
    fork_b = await client.fork_execution(
        parent_id=parent.request_id,
        command='echo "Fork B modification" >> /tmp/state.txt && cat /tmp/state.txt'
    )
    print(f"   Fork B output:\n{fork_b.output}\n")

    print("✅ Both forks started from the same parent state!\n")

    await client.session.close()


async def example_ml_workflow():
    """Demonstrate ML model serving workflow"""
    client = FaaSClient("http://localhost:8080")

    print("🤖 Machine Learning Workflow Example\n")

    # Pre-warm GPU-enabled containers
    print("1. Pre-warming GPU containers:")
    await client.prewarm("pytorch/pytorch:latest", count=2)
    print("   Ready for model inference\n")

    # Load model (simulated)
    print("2. Loading model:")
    model_code = '''
import json

# Simulate model loading
print("Loading model...")

# Simulate prediction
def predict(data):
    return {"prediction": "positive", "confidence": 0.95}

# Process input
input_data = {"text": "This is amazing!"}
result = predict(input_data)
print(json.dumps(result))
'''

    result = await client.run_python(model_code)
    print(f"   Model output: {result.output}\n")

    # Batch processing
    print("3. Batch inference:")
    batch_code = '''
import json

# Simulate batch processing
batch = [
    {"id": 1, "text": "Great product!"},
    {"id": 2, "text": "Not satisfied"},
    {"id": 3, "text": "Excellent service"}
]

results = []
for item in batch:
    # Simulate inference
    confidence = 0.9 if "great" in item["text"].lower() or "excellent" in item["text"].lower() else 0.3
    results.append({
        "id": item["id"],
        "prediction": "positive" if confidence > 0.5 else "negative",
        "confidence": confidence
    })

print(json.dumps(results, indent=2))
'''

    batch_result = await client.run_python(batch_code)
    print(f"   Batch results:\n{batch_result.output}\n")

    await client.session.close()


async def example_data_pipeline():
    """Demonstrate data processing pipeline"""
    client = FaaSClient("http://localhost:8080")

    print("📊 Data Pipeline Example\n")

    # Stage 1: Data extraction
    print("Stage 1: Extract data")
    extract = await client.run_python('''
import json

# Simulate data extraction
data = [
    {"id": i, "value": i * 10, "category": "A" if i % 2 == 0 else "B"}
    for i in range(1, 11)
]
print(json.dumps(data))
''')
    print(f"   Extracted {len(extract.output.split(','))} records\n")

    # Stage 2: Data transformation
    print("Stage 2: Transform data")
    transform = await client.run_python(f'''
import json

# Load extracted data
data = {extract.output}

# Transform data
transformed = []
for record in data:
    transformed.append({{
        "id": record["id"],
        "value": record["value"] * 1.1,  # Apply 10% increase
        "category": record["category"],
        "processed": True
    }})

print(json.dumps(transformed[:3]))  # Show first 3
print(f"Processed {{len(transformed)}} records")
''')
    print(f"   Transformation output:\n{transform.output}\n")

    # Stage 3: Data aggregation
    print("Stage 3: Aggregate results")
    aggregate = await client.run_python(f'''
import json

data = {extract.output}

# Aggregate by category
from collections import defaultdict
aggregates = defaultdict(lambda: {{"count": 0, "total": 0}})

for record in data:
    cat = record["category"]
    aggregates[cat]["count"] += 1
    aggregates[cat]["total"] += record["value"]

# Calculate averages
result = {{}}
for cat, stats in aggregates.items():
    result[cat] = {{
        "count": stats["count"],
        "total": stats["total"],
        "average": stats["total"] / stats["count"]
    }}

print(json.dumps(result, indent=2))
''')
    print(f"   Aggregation results:\n{aggregate.output}\n")

    await client.session.close()


async def example_streaming_logs():
    """Demonstrate log streaming"""
    client = FaaSClient("http://localhost:8080")

    print("📜 Log Streaming Example\n")

    # Start long-running execution
    print("Starting long-running task...")
    long_task = await client.execute(
        command='''
        for i in $(seq 1 5); do
            echo "Processing step $i..."
            sleep 1
        done
        echo "Task completed!"
        ''',
        image="alpine:latest"
    )

    print(f"Execution ID: {long_task.request_id}")
    print("Streaming logs:\n")

    # Stream logs (simulated - would be real-time in production)
    try:
        async for log_line in client.stream_logs(long_task.request_id):
            print(f"  LOG: {log_line}")
    except:
        # Streaming might not be available in all setups
        print("  (Log streaming not available in this environment)")
        if long_task.logs:
            print(f"  Batch logs:\n{long_task.logs}")

    await client.session.close()


async def example_firecracker_security():
    """Demonstrate Firecracker VM isolation for secure workloads"""
    client = FaaSClient("http://localhost:8080")

    print("🔒 Secure Execution with Firecracker VMs\n")

    # Configure for Firecracker runtime
    secure_client = FaaSClient(
        "http://localhost:8080"
    ).config.runtime = Runtime.FIRECRACKER

    print("1. Running sensitive computation in VM:")
    sensitive_code = '''
import hashlib

# Simulate processing sensitive data
sensitive_data = "credit_card_number_1234567890123456"
hashed = hashlib.sha256(sensitive_data.encode()).hexdigest()
print(f"Processed sensitive data: {hashed[:16]}...")

# Data is isolated in VM memory
print("Data remains isolated in VM")
'''

    result = await client.execute(
        command=f'python -c "{sensitive_code}"',
        image="python:3.11-slim",
        runtime=Runtime.FIRECRACKER
    )

    print(f"   Output: {result.output}")
    print(f"   Runtime: {result.runtime_used}")
    print("   ✅ Data processed in isolated VM environment\n")

    print("2. Multi-tenant isolation:")
    # Simulate multiple tenants
    for tenant_id in ["tenant-a", "tenant-b"]:
        tenant_result = await client.execute(
            command=f'echo "Processing data for {tenant_id}"',
            runtime=Runtime.FIRECRACKER,
            env_vars={"TENANT_ID": tenant_id}
        )
        print(f"   {tenant_id}: {tenant_result.output.strip()}")

    print("   ✅ Each tenant runs in isolated VM\n")

    await client.session.close()


async def main():
    """Run all advanced examples"""
    examples = [
        ("Forking", example_forking),
        ("ML Workflow", example_ml_workflow),
        ("Data Pipeline", example_data_pipeline),
        ("Log Streaming", example_streaming_logs),
        ("Firecracker Security", example_firecracker_security)
    ]

    print("=" * 60)
    print("FaaS Platform - Advanced Python Examples")
    print("=" * 60 + "\n")

    for name, example_func in examples:
        print(f"\n{'=' * 60}")
        print(f"Example: {name}")
        print("=" * 60 + "\n")
        try:
            await example_func()
        except Exception as e:
            print(f"⚠️ Example failed: {e}\n")

        await asyncio.sleep(1)  # Brief pause between examples

    print("\n" + "=" * 60)
    print("✅ All examples completed!")
    print("=" * 60)


if __name__ == "__main__":
    asyncio.run(main())