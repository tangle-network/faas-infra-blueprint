# Advanced Features Example

Comprehensive demo of FaaS platform capabilities including multi-language execution, caching, forking, and snapshots.

## Features Demonstrated

- ✅ Multi-language execution (Python, JavaScript, Bash)
- ✅ Execution forking and branching
- ✅ Snapshot management
- ✅ Intelligent caching
- ✅ Runtime selection (Docker/Firecracker)
- ✅ Resource limits (memory, CPU, timeout)
- ✅ Client-side metrics
- ✅ Health monitoring

## Running

```bash
# Start FaaS gateway server
cargo run --release --package faas-gateway-server

# Run the example
cargo run --release --package advanced-features
```

## Example Output

```
🚀 FaaS Platform Advanced Features Demo

1. Quick command execution:
   Output: Hello from FaaS!

2. Python execution:
   Python output: {
     "result": 42,
     "status": "computed",
     "python_version": "3.11.x"
   }

3. JavaScript execution:
   JavaScript output: {
     "result": 84,
     "timestamp": "2024-10-03T...",
     "node_version": "v20.x.x"
   }

4. Forked execution:
   Fork result: Forked execution completed

...
```

## Key Concepts

### Forking
Branch execution from a base state - perfect for A/B testing or parallel exploration:

```rust
let base = client.execute(/* base config */).await?;
let fork = client.fork_execution(&base.request_id, "new command").await?;
```

### Caching
Automatic result caching based on command + image + env:

```rust
let result = client.run_cached("echo 'cached'", "alpine:latest").await?;
```

### Snapshots
Create reusable environment snapshots:

```rust
let snapshot_id = client.create_snapshot(/* config */).await?;
let result = client.execute_from_snapshot(snapshot_id).await?;
```

## Lines of Code

112 lines showing the full SDK capabilities.
