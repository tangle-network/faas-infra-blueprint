# FaaS Tangle SDK (Python)

Python client for submitting jobs to the FaaS Blueprint on Tangle Network.

## Installation

```bash
pip install faas-tangle-sdk
```

## Requirements

- Python 3.8+
- substrate-interface (installed automatically)

## Quick Start

```python
from faas_tangle_sdk import TangleClient

async def main():
    # Connect to Tangle network
    client = await TangleClient.connect('ws://localhost:9944')

    # Submit Job 0: Execute Function
    result = await client.execute_function(
        image='alpine:latest',
        command=['echo', 'Hello from blockchain!'],
        env_vars=None,
        payload=b''
    )

    print(f'Job call ID: {result.call_id}')
    print(f'Output: {result.result.decode()}')
```

## Available Operations

### Job 0: Execute Function
```python
result = await client.execute_function(
    image='alpine:latest',
    command=['echo', 'test'],
    env_vars=['KEY=value'],
    payload=b''
)
```

### Job 1: Execute Advanced
```python
result = await client.execute_advanced(
    image='rust:latest',
    command=['cargo', 'build'],
    env_vars=None,
    payload=b'',
    mode='cached',
    checkpoint_id=None,
    branch_from=None,
    timeout_secs=60
)
```

### Job 2-11: Other Job Types
See full documentation for snapshot, instance, port, and file operations.

## Query Results

```python
# Get job result
result = await client.get_job_result(service_id, call_id)

# Check assigned operator
operator = await client.get_assigned_operator(call_id)
```

## Architecture

```
Python Client
     ↓ (substrate-interface)
Tangle Network (Substrate)
     ↓ (Smart Contract Call)
FaaSBlueprint Contract
     ↓ (Job Assignment)
Decentralized Operators
```

## Development Status

**Current**: Structure and types defined, implementation pending
**Next**: Full substrate-interface integration for contract interaction

## Related Packages

- `faas-sdk` - HTTP client for gateway server
- `faas-tangle-sdk` - Blockchain client (this package)
