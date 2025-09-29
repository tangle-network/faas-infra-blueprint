#!/usr/bin/env python3
"""
Comprehensive test suite for FaaS Python SDK

Tests all documented API methods:
- execute, run_python, run_javascript, run_bash
- fork_execution, prewarm, get_metrics, health_check
"""

import asyncio
import pytest
import aiohttp
from unittest.mock import AsyncMock, patch, MagicMock
import sys
import os

# Add the SDK to the path
sys.path.insert(0, os.path.dirname(__file__))

from faas_sdk import FaaSClient, Runtime, ExecutionResult, ExecutionMode


class TestFaaSSDK:

    @pytest.fixture
    def client(self):
        """Create a test client"""
        return FaaSClient("http://localhost:8080")

    @pytest.fixture
    def mock_response(self):
        """Mock HTTP response"""
        mock_resp = MagicMock()
        mock_resp.status = 200
        mock_resp.json = AsyncMock(return_value={
            "stdout": "test output",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 45,
            "request_id": "test-123"
        })
        return mock_resp

    @pytest.mark.asyncio
    async def test_execute_basic(self, client, mock_response):
        """Test basic execute functionality"""
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

    @pytest.mark.asyncio
    async def test_execute_with_working_dir(self, client, mock_response):
        """Test execute with working directory"""
        with patch.object(client.session, 'post', return_value=mock_response) as mock_post:
            result = await client.execute(
                command="pwd",
                working_dir="/app"
            )

            assert isinstance(result, ExecutionResult)
            mock_post.assert_called_once()

            # Check that working_dir was included in the request
            call_args = mock_post.call_args
            assert "working_dir" in str(call_args)

    @pytest.mark.asyncio
    async def test_run_python(self, client, mock_response):
        """Test run_python convenience method"""
        mock_response.json = AsyncMock(return_value={
            "stdout": "Hello from Python!\n42",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 67,
            "request_id": "python-test"
        })

        with patch.object(client.session, 'post', return_value=mock_response):
            code = '''
print("Hello from Python!")
result = 40 + 2
print(result)
'''
            result = await client.run_python(code)

            assert "Hello from Python!" in result.output
            assert "42" in result.output
            assert result.exit_code == 0

    @pytest.mark.asyncio
    async def test_run_javascript(self, client, mock_response):
        """Test run_javascript convenience method"""
        mock_response.json = AsyncMock(return_value={
            "stdout": "Hello from JavaScript!\n42",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 55,
            "request_id": "js-test"
        })

        with patch.object(client.session, 'post', return_value=mock_response):
            code = 'console.log("Hello from JavaScript!"); console.log(42);'
            result = await client.run_javascript(code)

            assert "Hello from JavaScript!" in result.output
            assert "42" in result.output
            assert result.exit_code == 0

    @pytest.mark.asyncio
    async def test_run_bash(self, client, mock_response):
        """Test run_bash convenience method"""
        mock_response.json = AsyncMock(return_value={
            "stdout": "Hello from Bash!\nCurrent date: 2024-01-15",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 30,
            "request_id": "bash-test"
        })

        with patch.object(client.session, 'post', return_value=mock_response):
            script = 'echo "Hello from Bash!"; echo "Current date: $(date +%Y-%m-%d)"'
            result = await client.run_bash(script)

            assert "Hello from Bash!" in result.output
            assert result.exit_code == 0

    @pytest.mark.asyncio
    async def test_prewarm(self, client, mock_response):
        """Test prewarm functionality"""
        mock_response.json = AsyncMock(return_value={
            "message": "Pre-warmed 3 containers",
            "containers_created": 3
        })

        with patch.object(client.session, 'post', return_value=mock_response):
            await client.prewarm("python:3.11-slim", count=3)
            # Should not raise an exception

    @pytest.mark.asyncio
    async def test_get_metrics(self, client, mock_response):
        """Test get_metrics functionality"""
        mock_response.json = AsyncMock(return_value={
            "total_executions": 1547,
            "average_latency_ms": 87.5,
            "cache_hit_rate": 0.73,
            "active_containers": 15
        })

        with patch.object(client.session, 'get', return_value=mock_response):
            metrics = await client.get_metrics()

            assert isinstance(metrics, dict)
            assert metrics["total_executions"] > 0
            assert metrics["cache_hit_rate"] > 0.0

    @pytest.mark.asyncio
    async def test_health_check(self, client, mock_response):
        """Test health_check functionality"""
        mock_response.json = AsyncMock(return_value={
            "status": "healthy",
            "version": "1.0.0",
            "components": {
                "executor": "healthy",
                "docker": "healthy"
            }
        })

        with patch.object(client.session, 'get', return_value=mock_response):
            health = await client.health_check()

            assert isinstance(health, dict)
            assert health["status"] == "healthy"

    @pytest.mark.asyncio
    async def test_fork_execution(self, client, mock_response):
        """Test fork_execution functionality"""
        mock_response.json = AsyncMock(return_value={
            "stdout": "Forked execution result",
            "stderr": "",
            "exit_code": 0,
            "duration_ms": 25,
            "request_id": "fork-test"
        })

        with patch.object(client.session, 'post', return_value=mock_response):
            result = await client.fork_execution("parent-123", "echo 'Forked execution'")

            assert "Forked execution" in result.output
            assert result.exit_code == 0

    @pytest.mark.asyncio
    async def test_error_handling(self, client):
        """Test error handling"""
        mock_resp = MagicMock()
        mock_resp.status = 500
        mock_resp.json = AsyncMock(return_value={
            "error": "Internal server error"
        })

        with patch.object(client.session, 'post', return_value=mock_resp):
            with pytest.raises(Exception):  # Should raise an exception
                await client.execute("exit 1")

    def test_client_creation(self):
        """Test client creation with different parameters"""
        # Test basic creation
        client1 = FaaSClient("http://localhost:8080")
        assert client1.config.base_url == "http://localhost:8080"

        # Test with runtime
        client2 = FaaSClient("http://localhost:8080", runtime=Runtime.DOCKER)
        assert client2.config.runtime == Runtime.DOCKER

    def test_runtime_enum(self):
        """Test Runtime enum values"""
        assert Runtime.DOCKER.value == "docker"
        assert Runtime.FIRECRACKER.value == "firecracker"
        assert Runtime.AUTO.value == "auto"

    def test_execution_mode_enum(self):
        """Test ExecutionMode enum values"""
        assert ExecutionMode.NORMAL
        assert ExecutionMode.CACHED
        assert ExecutionMode.BRANCHED

    @pytest.mark.asyncio
    async def test_env_vars_support(self, client, mock_response):
        """Test environment variables support"""
        with patch.object(client.session, 'post', return_value=mock_response):
            result = await client.execute(
                command="echo $TEST_VAR",
                env_vars={"TEST_VAR": "production"}
            )

            # Just ensure it doesn't raise an exception
            assert isinstance(result, ExecutionResult)

    def test_cache_key_generation(self, client):
        """Test cache key generation"""
        key1 = client._get_cache_key("test code")
        key2 = client._get_cache_key("test code")
        key3 = client._get_cache_key("different code")

        assert key1 == key2  # Same input should give same key
        assert key1 != key3  # Different input should give different key
        assert len(key1) == 32  # MD5 hash length

    def test_client_metrics(self, client):
        """Test client metrics functionality"""
        metrics = client.get_client_metrics()

        assert hasattr(metrics, 'total_requests')
        assert hasattr(metrics, 'cache_hit_rate')
        assert hasattr(metrics, 'error_rate')
        assert metrics.total_requests == 0  # New client should have 0 requests


if __name__ == "__main__":
    pytest.main([__file__, "-v"])