"""
Integration tests for Python SDK

These tests run against a real faas-gateway-server instance.

Prerequisites:
- faas-gateway-server must be running on localhost:8080
- Docker must be available for container execution

To run:
1. Start the gateway: cargo run --package faas-gateway-server --release
2. Run tests: pytest tests/test_integration.py -v
"""

import asyncio
import os
import pytest

import sys
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))

from faas_sdk import FaaSClient, Runtime

GATEWAY_URL = os.getenv('FAAS_GATEWAY_URL', 'http://localhost:8080')
TEST_TIMEOUT = 30  # seconds


@pytest.fixture
def client():
    """Create a FaaS client instance for integration testing."""
    return FaaSClient(GATEWAY_URL)


@pytest.mark.asyncio
async def test_health_check(client):
    """Verify gateway is running and healthy."""
    health = await client.health()
    assert health is not None
    assert 'status' in health


@pytest.mark.asyncio
async def test_basic_execution(client):
    """Test simple command execution."""
    result = await client.execute(
        command='echo "Hello from Docker"',
        image='alpine:latest'
    )

    assert result.output is not None
    assert 'Hello from Docker' in result.output
    assert result.request_id is not None
    assert result.duration_ms > 0


@pytest.mark.asyncio
async def test_execution_with_env_vars(client):
    """Test execution with environment variables."""
    result = await client.execute(
        command='sh -c "echo $MY_VAR"',
        image='alpine:latest',
        env_vars={'MY_VAR': 'test-value'}
    )

    assert 'test-value' in result.output


@pytest.mark.asyncio
async def test_run_python(client):
    """Test Python code execution."""
    code = '''
print("Python works")
result = 2 + 2
print(result)
'''
    result = await client.run_python(code)

    assert 'Python works' in result.output
    assert '4' in result.output


@pytest.mark.asyncio
async def test_run_javascript(client):
    """Test JavaScript code execution."""
    code = '''
console.log("JavaScript works");
console.log(2 + 2);
'''
    result = await client.run_javascript(code)

    assert 'JavaScript works' in result.output
    assert '4' in result.output


@pytest.mark.asyncio
async def test_run_bash(client):
    """Test Bash script execution."""
    script = 'echo "Bash works"; expr 2 + 2'
    result = await client.run_bash(script)

    assert 'Bash works' in result.output
    assert '4' in result.output


@pytest.mark.asyncio
async def test_runtime_selection(client):
    """Test explicit runtime selection."""
    result = await client.execute(
        command='echo "Docker runtime"',
        image='alpine:latest',
        runtime=Runtime.DOCKER
    )

    assert 'Docker runtime' in result.output


@pytest.mark.asyncio
async def test_performance(client):
    """Test that execution completes in reasonable time."""
    import time

    start = time.time()
    await client.execute(
        command='echo "Performance test"',
        image='alpine:latest'
    )
    elapsed = time.time() - start

    assert elapsed < 10.0  # Should complete within 10 seconds


@pytest.mark.asyncio
async def test_error_handling(client):
    """Test error handling for failed commands."""
    # This should complete but may have non-zero exit code
    result = await client.execute(
        command='sh -c "echo error && exit 1"',
        image='alpine:latest'
    )

    assert result.request_id is not None


@pytest.mark.asyncio
async def test_timeout(client):
    """Test timeout handling."""
    with pytest.raises(Exception):
        await client.execute(
            command='sleep 60',
            image='alpine:latest',
            timeout_ms=1000
        )


@pytest.mark.asyncio
async def test_get_metrics(client):
    """Test retrieving server metrics."""
    metrics = await client.get_metrics()

    assert metrics is not None
    assert isinstance(metrics, dict)


@pytest.mark.asyncio
async def test_concurrent_executions(client):
    """Test concurrent execution requests."""
    tasks = [
        client.execute(
            command=f'echo "Test {i}"',
            image='alpine:latest'
        )
        for i in range(3)
    ]

    results = await asyncio.gather(*tasks)

    assert len(results) == 3
    for i, result in enumerate(results):
        assert f'Test {i}' in result.output


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
