# Data Pipeline Example

ETL (Extract, Transform, Load) workflow demonstrating data processing capabilities.

## Prerequisites

- **Docker** - Required for container execution
  - Download from: https://www.docker.com/products/docker-desktop
- **FaaS Gateway Server** - Must be running on port 8080
- **Docker Images** - The following images will be pulled automatically:
  - `alpine:latest` (for data extraction and loading)
  - `python:3.11-slim` (for pandas transformations)

## Features

- ✅ Multi-stage data processing (Extract → Transform → Load)
- ✅ CSV data extraction
- ✅ Real pandas transformations (groupby, aggregation, derived metrics)
- ✅ JSON serialization of complex DataFrames
- ✅ Execution modes for optimization
- ✅ Error handling and proper output parsing
- ✅ Log stream analysis with pattern matching

## Running

```bash
# 1. Start FaaS gateway server (in one terminal)
cargo run --release --package faas-gateway-server

# 2. Wait for gateway to start (look for "listening on 0.0.0.0:8080")

# 3. Run the data pipeline (in another terminal)
cargo run --release --package data-pipeline
```

## Note

First run will take a few minutes as:
- Docker images are pulled (~200MB for Python image)
- Pandas is installed inside the container (takes ~30-60 seconds)

Subsequent runs will be faster as the images are cached.

## Pipeline Stages

1. **Extract**: Load data from CSV source
2. **Transform**: Clean, validate, and enrich data
3. **Aggregate**: Calculate statistics and summaries
4. **Load**: Output to multiple formats (JSON, Parquet)

## Use Cases

- Data ingestion workflows
- ETL batch processing
- Data quality validation
- Report generation

## Lines of Code

292 lines - Complete ETL workflow
