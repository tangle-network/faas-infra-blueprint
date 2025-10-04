# Quickstart Example

Minimal working demo showing basic FaaS SDK usage.

## Features

- Simple command execution
- Data processing via stdin
- Environment variables

## Running

```bash
# Start FaaS gateway server first
cargo run --release --package faas-gateway-server

# In another terminal, run the example
cargo run --release --package quickstart
```

## What It Does

1. Executes a simple echo command in Alpine container
2. Processes data with `wc -l` 
3. Uses environment variables to pass configuration

## Code Size

44 lines - Perfect for getting started!

## Next Steps

Check out `advanced-features` for more complex examples.
