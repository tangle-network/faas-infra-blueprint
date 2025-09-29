#!/usr/bin/env python3
"""Test script to verify gateway functionality with SDK"""

import sys
import os
import time

# Add SDK to path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'sdk/python'))

from faas_sdk.client import FaaSClient

def test_gateway():
    print("🧪 Testing FaaS Gateway...\n")

    # Create client
    client = FaaSClient(base_url="http://localhost:8080")

    # Test 1: Basic execution
    print("1. Testing basic execution...")
    try:
        result = client.execute(
            command="echo 'Hello from FaaS!'",
            image="alpine:latest"
        )
        print(f"   ✅ Result: {result.stdout.strip()}")
        print(f"   ⏱️  Duration: {result.duration_ms}ms")
    except Exception as e:
        print(f"   ❌ Failed: {e}")

    # Test 2: Advanced execution modes
    print("\n2. Testing execution modes...")
    modes = ["ephemeral", "cached"]

    for mode in modes:
        print(f"   Testing {mode} mode...")
        try:
            result = client.execute_advanced(
                command="date +%s%N",
                image="alpine:latest",
                mode=mode
            )
            print(f"   ✅ {mode}: {result.duration_ms}ms")
        except Exception as e:
            print(f"   ❌ {mode} failed: {e}")

    # Test 3: Snapshot management
    print("\n3. Testing snapshots...")
    try:
        snapshots = client.snapshots.list()
        print(f"   📸 Found {len(snapshots)} snapshots")
    except Exception as e:
        print(f"   ❌ Failed to list snapshots: {e}")

    # Test 4: Instance management
    print("\n4. Testing instances...")
    try:
        instance = client.instances.start(
            image="alpine:latest",
            cpu_cores=1,
            memory_mb=512
        )
        print(f"   ✅ Created instance: {instance.id}")

        # Execute in instance
        result = instance.exec("uname -a")
        print(f"   ✅ Exec result: {result.stdout.strip()}")

        # Stop instance
        instance.stop()
        print(f"   ✅ Stopped instance")
    except Exception as e:
        print(f"   ❌ Instance test failed: {e}")

    print("\n✨ Gateway tests complete!")

if __name__ == "__main__":
    # Check if gateway is running
    import requests
    try:
        r = requests.get("http://localhost:8080/health")
        if r.status_code == 200:
            print("✅ Gateway is healthy\n")
            test_gateway()
        else:
            print("❌ Gateway health check failed")
    except:
        print("❌ Gateway is not running. Start it with: ./start-gateway.sh")
        sys.exit(1)