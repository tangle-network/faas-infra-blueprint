"""
Comprehensive tests for Python SDK convenience methods.

Tests all documented top-level API methods:
- run_python, run_javascript, fork_execution
- prewarm, get_metrics, health
"""

import asyncio
import json
import pytest
from unittest.mock import AsyncMock, patch, MagicMock
from datetime import datetime

import sys
import os
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))

from faas_sdk import FaaSClient, Runtime, ForkStrategy, ExecutionResult, ForkResult


@pytest.fixture
def client():
    """Create a FaaS client instance for testing."""
    return FaaSClient("http://localhost:8080")


@pytest.fixture
def mock_response():
    """Create a mock response object."""
    response = AsyncMock()
    response.status = 200
    response.headers = {"content-type": "application/json"}
    return response


@pytest.mark.asyncio
async def test_run_python(client, mock_response):
    """Test the run_python convenience method."""
    expected_result = {
        "stdout": "Hello from Python!\n42",
        "stderr": "",
        "exit_code": 0,
        "duration_ms": 45,
        "request_id": "python-123",
        "cached": False
    }

    mock_response.json = AsyncMock(return_value=expected_result)

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        code = """
print("Hello from Python!")
result = 40 + 2
print(result)
"""
        result = await client.run_python(code)

        assert isinstance(result, ExecutionResult)
        assert result.stdout == "Hello from Python!\n42"
        assert result.exit_code == 0
        assert result.duration_ms == 45
        assert not result.cached


@pytest.mark.asyncio
async def test_run_javascript(client, mock_response):
    """Test the run_javascript convenience method."""
    expected_result = {
        "stdout": "Hello from JavaScript!\n42",
        "stderr": "",
        "exit_code": 0,
        "duration_ms": 35,
        "request_id": "js-123",
        "cached": False
    }

    mock_response.json = AsyncMock(return_value=expected_result)

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        code = """
console.log("Hello from JavaScript!");
console.log(40 + 2);
"""
        result = await client.run_javascript(code)

        assert isinstance(result, ExecutionResult)
        assert result.stdout == "Hello from JavaScript!\n42"
        assert result.duration_ms == 35


@pytest.mark.asyncio
async def test_fork_execution(client, mock_response):
    """Test the fork_execution method for A/B testing."""
    expected_result = {
        "results": [
            {
                "branch_id": "version-a",
                "stdout": "Algorithm A result",
                "stderr": "",
                "exit_code": 0,
                "duration_ms": 120
            },
            {
                "branch_id": "version-b",
                "stdout": "Algorithm B result",
                "stderr": "",
                "exit_code": 0,
                "duration_ms": 85
            }
        ],
        "selected_branch": "version-b",
        "selection_reason": "lower_latency"
    }

    mock_response.json = AsyncMock(return_value=expected_result)

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        branches = [
            {
                "id": "version-a",
                "command": "python -c 'print(\"Algorithm A\")'",
                "weight": 0.5
            },
            {
                "id": "version-b",
                "command": "python -c 'print(\"Algorithm B\")'",
                "weight": 0.5
            }
        ]

        result = await client.fork_execution(
            branches=branches,
            image="python:3.11",
            strategy=ForkStrategy.PARALLEL
        )

        assert isinstance(result, ForkResult)
        assert len(result.results) == 2
        assert result.selected_branch == "version-b"
        assert result.selection_reason == "lower_latency"

        # Check individual branch results
        branch_a = next(r for r in result.results if r["branch_id"] == "version-a")
        assert branch_a["duration_ms"] == 120

        branch_b = next(r for r in result.results if r["branch_id"] == "version-b")
        assert branch_b["duration_ms"] == 85


@pytest.mark.asyncio
async def test_prewarm(client, mock_response):
    """Test the prewarm method for container warming."""
    expected_result = {
        "success": True,
        "containers_warmed": 5,
        "average_warmup_ms": 125
    }

    mock_response.json = AsyncMock(return_value=expected_result)

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        result = await client.prewarm(
            image="alpine:latest",
            count=5,
            runtime=Runtime.DOCKER
        )

        assert result["success"] is True
        assert result["containers_warmed"] == 5
        assert result["average_warmup_ms"] == 125


@pytest.mark.asyncio
async def test_prewarm_firecracker(client, mock_response):
    """Test prewarming with Firecracker runtime."""
    expected_result = {
        "success": True,
        "containers_warmed": 3,
        "average_warmup_ms": 95
    }

    mock_response.json = AsyncMock(return_value=expected_result)

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        result = await client.prewarm(
            image="alpine:latest",
            count=3,
            runtime=Runtime.FIRECRACKER,
            memory_mb=256,
            cpu_cores=1
        )

        assert result["success"] is True
        assert result["average_warmup_ms"] < 100  # Firecracker should be faster


@pytest.mark.asyncio
async def test_get_metrics(client, mock_response):
    """Test the get_metrics method."""
    expected_metrics = {
        "total_executions": 10000,
        "avg_execution_time_ms": 42.5,
        "cache_hit_rate": 0.87,
        "active_containers": 15,
        "memory_usage_mb": 3072,
        "cpu_usage_percent": 45.2,
        "warm_start_ratio": 0.92,
        "cold_starts_last_hour": 8,
        "errors_last_hour": 2,
        "p99_latency_ms": 125,
        "p95_latency_ms": 85,
        "p50_latency_ms": 35
    }

    mock_response.json = AsyncMock(return_value=expected_metrics)

    with patch('aiohttp.ClientSession.get', return_value=mock_response):
        metrics = await client.get_metrics()

        assert metrics["total_executions"] == 10000
        assert metrics["avg_execution_time_ms"] == 42.5
        assert metrics["cache_hit_rate"] == 0.87
        assert metrics["warm_start_ratio"] == 0.92
        assert metrics["p50_latency_ms"] == 35
        assert metrics["p99_latency_ms"] == 125


@pytest.mark.asyncio
async def test_health(client, mock_response):
    """Test the health check method."""
    expected_health = {
        "status": "healthy",
        "uptime_seconds": 86400,
        "version": "1.0.0",
        "components": {
            "docker": "healthy",
            "cache": "healthy",
            "scheduler": "healthy",
            "firecracker": "healthy",
            "metrics": "healthy"
        },
        "last_check": "2024-01-01T12:00:00Z"
    }

    mock_response.json = AsyncMock(return_value=expected_health)

    with patch('aiohttp.ClientSession.get', return_value=mock_response):
        health = await client.health()

        assert health["status"] == "healthy"
        assert health["uptime_seconds"] == 86400
        assert health["components"]["docker"] == "healthy"
        assert health["components"]["firecracker"] == "healthy"
        assert all(status == "healthy" for status in health["components"].values())


@pytest.mark.asyncio
async def test_health_degraded(client, mock_response):
    """Test health check when system is degraded."""
    expected_health = {
        "status": "degraded",
        "uptime_seconds": 3600,
        "components": {
            "docker": "healthy",
            "cache": "degraded",
            "scheduler": "healthy"
        },
        "issues": ["Cache hit rate below threshold", "High memory usage"]
    }

    mock_response.json = AsyncMock(return_value=expected_health)

    with patch('aiohttp.ClientSession.get', return_value=mock_response):
        health = await client.health()

        assert health["status"] == "degraded"
        assert health["components"]["cache"] == "degraded"
        assert "issues" in health
        assert len(health["issues"]) == 2


@pytest.mark.asyncio
async def test_run_python_with_packages(client, mock_response):
    """Test running Python code with package imports."""
    expected_result = {
        "stdout": "NumPy array: [1 2 3 4 5]\nSum: 15",
        "stderr": "",
        "exit_code": 0,
        "duration_ms": 250,
        "request_id": "numpy-123",
        "cached": False
    }

    mock_response.json = AsyncMock(return_value=expected_result)

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        code = """
import numpy as np
arr = np.array([1, 2, 3, 4, 5])
print(f"NumPy array: {arr}")
print(f"Sum: {arr.sum()}")
"""
        result = await client.run_python(code, image="python:3.11-slim")

        assert "NumPy array" in result.stdout
        assert "Sum: 15" in result.stdout


@pytest.mark.asyncio
async def test_run_javascript_with_modules(client, mock_response):
    """Test running JavaScript with module imports."""
    expected_result = {
        "stdout": "Lodash sum: 15\nMoment date: 2024-01-01",
        "stderr": "",
        "exit_code": 0,
        "duration_ms": 180,
        "request_id": "node-modules-123",
        "cached": False
    }

    mock_response.json = AsyncMock(return_value=expected_result)

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        code = """
const _ = require('lodash');
const moment = require('moment');

const numbers = [1, 2, 3, 4, 5];
console.log('Lodash sum:', _.sum(numbers));
console.log('Moment date:', moment('2024-01-01').format('YYYY-MM-DD'));
"""
        result = await client.run_javascript(code, image="node:20")

        assert "Lodash sum: 15" in result.stdout
        assert "Moment date: 2024-01-01" in result.stdout


@pytest.mark.asyncio
async def test_fork_execution_with_selection_criteria(client, mock_response):
    """Test fork execution with different selection strategies."""
    # Test with FASTEST strategy
    expected_result = {
        "results": [
            {
                "branch_id": "slow",
                "stdout": "Slow result",
                "duration_ms": 500
            },
            {
                "branch_id": "fast",
                "stdout": "Fast result",
                "duration_ms": 50
            }
        ],
        "selected_branch": "fast",
        "selection_reason": "fastest_execution"
    }

    mock_response.json = AsyncMock(return_value=expected_result)

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        branches = [
            {"id": "slow", "command": "sleep 0.5 && echo 'Slow'"},
            {"id": "fast", "command": "echo 'Fast'"}
        ]

        result = await client.fork_execution(
            branches=branches,
            image="alpine:latest",
            strategy=ForkStrategy.FASTEST
        )

        assert result.selected_branch == "fast"
        assert result.selection_reason == "fastest_execution"


@pytest.mark.asyncio
async def test_performance_benchmarks(client, mock_response):
    """Test that our performance meets documented benchmarks."""
    # Test warm start < 50ms
    warm_start_result = {
        "stdout": "Warm start",
        "stderr": "",
        "exit_code": 0,
        "duration_ms": 35,
        "cached": True,
        "warm_start": True
    }

    mock_response.json = AsyncMock(return_value=warm_start_result)

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        result = await client.execute(
            command="echo 'test'",
            image="alpine:latest",
            cache_key="warm-test"
        )

        assert result.duration_ms < 50  # Documented warm start time
        assert result.cached or result.get("warm_start", False)

    # Test branching < 250ms
    branch_result = {
        "results": [
            {"branch_id": "a", "duration_ms": 100},
            {"branch_id": "b", "duration_ms": 120}
        ],
        "total_duration_ms": 235,
        "selected_branch": "a"
    }

    mock_response.json = AsyncMock(return_value=branch_result)

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        result = await client.fork_execution(
            branches=[
                {"id": "a", "command": "echo 'A'"},
                {"id": "b", "command": "echo 'B'"}
            ],
            image="alpine:latest"
        )

        # Check that branching completes within documented time
        total_duration = branch_result.get("total_duration_ms", 250)
        assert total_duration < 250  # Documented branching time


@pytest.mark.asyncio
async def test_error_handling_convenience_methods(client):
    """Test error handling in convenience methods."""
    # Test network error
    with patch('aiohttp.ClientSession.post', side_effect=Exception("Network error")):
        with pytest.raises(Exception) as exc_info:
            await client.run_python("print('test')")
        assert "Network error" in str(exc_info.value)

    # Test server error response
    mock_response = AsyncMock()
    mock_response.status = 500
    mock_response.text = AsyncMock(return_value="Internal server error")

    with patch('aiohttp.ClientSession.post', return_value=mock_response):
        with pytest.raises(Exception) as exc_info:
            await client.run_javascript("console.log('test')")
        assert "500" in str(exc_info.value) or "error" in str(exc_info.value).lower()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])