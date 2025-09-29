# FaaS Platform Python SDK

[![PyPI](https://img.shields.io/pypi/v/faas-sdk.svg)](https://pypi.org/project/faas-sdk/)
[![Documentation](https://img.shields.io/badge/docs-latest-brightgreen.svg)](https://docs.faas-platform.com/python-sdk)

Official Python SDK for the FaaS Platform with async/await support and intelligent optimization.

## Features

- 🚀 **Dual Runtime Support**: Docker containers and Firecracker microVMs
- 📊 **Smart Caching**: Automatic result caching with configurable TTL
- 🔥 **Pre-warming**: Zero cold starts with warm container pools
- 🌳 **Execution Forking**: Branch workflows for A/B testing
- 📈 **Auto-scaling**: Predictive scaling based on load patterns
- 📋 **Rich Metrics**: Built-in performance monitoring
- 🔄 **Async/Await**: Full asyncio support with connection pooling

## Installation

```bash
pip install faas-sdk
```

## Quick Start

```python
import asyncio
from faas_sdk import FaaSClient

async def main():
    client = FaaSClient("http://localhost:8080")

    # Simple Python execution
    result = await client.run_python('print("Hello, World!")')
    print(result.output)

    # Advanced execution
    result = await client.execute(
        command="python ml_inference.py",
        image="pytorch/pytorch:latest",
        env_vars={"MODEL_PATH": "/models/bert"},
        timeout_ms=60000
    )
    print(f"Execution took {result.duration_ms}ms")

asyncio.run(main())
```

## API Reference

### FaaSClient

The main client class for interacting with the FaaS platform.

#### Methods

- `run_python(code: str)` - Execute Python code directly
- `run_javascript(code: str)` - Execute JavaScript/Node.js code
- `run_bash(script: str)` - Execute bash scripts
- `execute(command: str, **kwargs)` - General-purpose execution
- `execute_advanced(request: dict)` - Advanced execution with all options
- `fork_execution(parent_id: str, command: str)` - Fork existing execution
- `prewarm(image: str, count: int)` - Pre-warm containers
- `get_metrics()` - Get server performance metrics
- `health_check()` - Check platform health

### Runtime Selection

```python
from faas_sdk import Runtime

# Development with Docker (fastest iteration)
client = FaaSClient("http://localhost:8080", runtime=Runtime.DOCKER)

# Production with Firecracker (strongest security)
client = FaaSClient("https://api.example.com", runtime=Runtime.FIRECRACKER)

# Automatic selection
client = FaaSClient("http://localhost:8080", runtime=Runtime.AUTO)
```

### Execution Modes

```python
from faas_sdk import ExecutionMode

# Cached execution for repeated requests
result = await client.execute_advanced({
    'command': 'python inference.py',
    'mode': ExecutionMode.CACHED
})

# Persistent service
result = await client.execute_advanced({
    'command': 'python server.py',
    'mode': ExecutionMode.PERSISTENT
})
```

## Examples

See the [examples directory](../../examples/python/) for complete examples:

- [quickstart.py](../../examples/python/quickstart.py) - Basic usage patterns
- [advanced.py](../../examples/python/advanced.py) - Advanced workflows and features

## Performance Tips

1. **Use caching** for deterministic computations
2. **Pre-warm containers** for critical paths
3. **Use Firecracker** for production workloads requiring isolation
4. **Monitor metrics** to identify bottlenecks
5. **Batch operations** when possible

## Error Handling

```python
import aiohttp
import asyncio

try:
    result = await client.execute(command="invalid_command")
except aiohttp.ClientError as e:
    print(f"Network error: {e}")
except asyncio.TimeoutError:
    print("Request timed out")
except Exception as e:
    print(f"Execution failed: {e}")
```

## License

This project is licensed under the MIT License.