# Data Pipeline Example

ETL (Extract, Transform, Load) workflow demonstrating data processing capabilities.

## Features

- ✅ Multi-stage data processing
- ✅ CSV data extraction
- ✅ Data transformation and enrichment
- ✅ Multiple output formats (JSON, Parquet)
- ✅ Execution modes for optimization
- ✅ Error handling and retry logic

## Running

```bash
# Start FaaS gateway server
cargo run --release --package faas-gateway-server

# Run the data pipeline
cargo run --release --package data-pipeline
```

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
