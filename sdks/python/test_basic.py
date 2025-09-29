#!/usr/bin/env python3
"""
Basic test suite for FaaS Python SDK without external dependencies
"""

import asyncio
from unittest.mock import AsyncMock, patch, MagicMock
import sys
import os

# Add the SDK to the path
sys.path.insert(0, os.path.dirname(__file__))

from faas_sdk import FaaSClient, Runtime, ExecutionResult, ExecutionMode


def test_client_creation():
    """Test client creation with different parameters"""
    # Test basic creation
    client1 = FaaSClient("http://localhost:8080")
    assert client1.config.base_url == "http://localhost:8080"

    # Test with custom config
    from faas_sdk import ClientConfig
    config = ClientConfig(base_url="http://localhost:8080", runtime=Runtime.DOCKER)
    client2 = FaaSClient("http://localhost:8080", config=config)
    assert client2.config.runtime == Runtime.DOCKER
    print("✅ Client creation tests passed")


def test_runtime_enum():
    """Test Runtime enum values"""
    assert Runtime.DOCKER.value == "docker"
    assert Runtime.FIRECRACKER.value == "firecracker"
    assert Runtime.AUTO.value == "auto"
    print("✅ Runtime enum tests passed")


def test_execution_mode_enum():
    """Test ExecutionMode enum values"""
    assert ExecutionMode.EPHEMERAL
    assert ExecutionMode.CACHED
    assert ExecutionMode.BRANCHED
    assert ExecutionMode.CHECKPOINTED
    assert ExecutionMode.PERSISTENT
    print("✅ ExecutionMode enum tests passed")


def test_cache_key_generation():
    """Test cache key generation"""
    client = FaaSClient("http://localhost:8080")

    key1 = client._get_cache_key("test code")
    key2 = client._get_cache_key("test code")
    key3 = client._get_cache_key("different code")

    assert key1 == key2  # Same input should give same key
    assert key1 != key3  # Different input should give different key
    assert len(key1) == 32  # MD5 hash length
    print("✅ Cache key generation tests passed")


def test_client_metrics():
    """Test client metrics functionality"""
    client = FaaSClient("http://localhost:8080")
    metrics = client.get_client_metrics()

    assert hasattr(metrics, 'total_requests')
    assert hasattr(metrics, 'cache_hit_rate')
    assert hasattr(metrics, 'error_rate')
    assert metrics.total_requests == 0  # New client should have 0 requests
    print("✅ Client metrics tests passed")


async def test_execute_basic():
    """Test basic execute functionality"""
    client = FaaSClient("http://localhost:8080")

    # Mock the HTTP session
    mock_response = MagicMock()
    mock_response.status = 200
    mock_response.json = AsyncMock(return_value={
        "stdout": "test output",
        "stderr": "",
        "exit_code": 0,
        "duration_ms": 45,
        "request_id": "test-123"
    })

    with patch.object(client.session, 'post', return_value=mock_response) as mock_post:
        result = await client.execute(
            command="echo test",
            image="alpine:latest"
        )

        assert isinstance(result, ExecutionResult)
        assert result.output == "test output"
        assert result.exit_code == 0
        assert result.duration_ms == 45
        mock_post.assert_called_once()

    print("✅ Basic execute test passed")


async def run_async_tests():
    """Run async tests"""
    await test_execute_basic()


def main():
    """Run all tests"""
    print("🧪 Running FaaS Python SDK Basic Tests")
    print("=" * 50)

    # Run sync tests
    test_client_creation()
    test_runtime_enum()
    test_execution_mode_enum()
    test_cache_key_generation()
    test_client_metrics()

    # Run async tests
    asyncio.run(run_async_tests())

    print("=" * 50)
    print("✅ All tests passed!")


if __name__ == "__main__":
    main()