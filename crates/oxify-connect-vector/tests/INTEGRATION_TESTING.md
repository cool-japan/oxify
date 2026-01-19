# Integration Testing Guide

This guide explains how to run integration tests for `oxify-connect-vector` against real vector databases.

## Overview

Integration tests verify that the vector database providers work correctly with actual database instances. These tests are:
- **Ignored by default** to avoid requiring database dependencies for regular test runs
- **Run against Docker containers** for consistency and reproducibility
- **Sequential** (use `--test-threads=1`) to avoid conflicts between tests

## Prerequisites

- Docker and Docker Compose installed
- At least 4GB of free RAM for running test databases
- Ports 5432, 6333, 6334, 8000, and 19530 available

## Quick Start

### 1. Start Test Databases

```bash
# From the oxify-connect-vector directory
docker-compose up -d
```

This starts:
- **Qdrant** on ports 6333 (HTTP) and 6334 (gRPC)
- **PostgreSQL with pgvector** on port 5432
- **ChromaDB** on port 8000
- **Milvus** on ports 19530 and 9091

### 2. Wait for Databases to be Ready

```bash
# Wait ~10-15 seconds for all services to initialize
sleep 15

# Or check individual service health
docker-compose ps
```

### 3. Run Integration Tests

```bash
# Run all integration tests
cargo test --test integration_test -- --ignored --test-threads=1

# Run specific provider tests
cargo test --test integration_test test_qdrant_integration -- --ignored
cargo test --test integration_test test_pgvector_integration -- --ignored
cargo test --test integration_test test_chromadb_integration -- --ignored

# Run with output
cargo test --test integration_test -- --ignored --test-threads=1 --nocapture
```

### 4. Cleanup

```bash
# Stop and remove containers, volumes, and networks
docker-compose down -v
```

## Environment Variables

You can customize database connection URLs using environment variables:

```bash
# Custom Qdrant URL
export QDRANT_URL=http://localhost:6333

# Custom PostgreSQL URL
export POSTGRES_URL=postgres://test:test@localhost/vectordb

# Custom ChromaDB URL
export CHROMADB_URL=http://localhost:8000

# Custom Milvus URL
export MILVUS_URL=http://localhost:19530

# Run tests with custom URLs
cargo test --test integration_test -- --ignored --test-threads=1
```

## Available Integration Tests

### Provider Tests

1. **`test_qdrant_integration`** - Tests Qdrant provider
   - Collection CRUD operations
   - Vector insert, search, update, delete
   - Batch operations
   - Collection info

2. **`test_pgvector_integration`** - Tests pgvector provider
   - PostgreSQL with vector extension
   - All CRUD operations
   - Batch inserts
   - SQL-based filtering

3. **`test_chromadb_integration`** - Tests ChromaDB provider
   - Collection management
   - Vector operations
   - Metadata filtering

### Feature Tests

4. **`test_hybrid_search_integration`** - Tests hybrid search
   - Combines semantic and keyword search
   - BM25 + vector search fusion
   - Reciprocal rank fusion (RRF)

5. **`test_colbert_integration`** - Tests ColBERT multi-vector search
   - Multi-vector document insertion
   - MaxSim scoring
   - Late interaction search

## Troubleshooting

### Port Conflicts

If you see "port already in use" errors:

```bash
# Check what's using the ports
lsof -i :6333  # Qdrant
lsof -i :5432  # PostgreSQL
lsof -i :8000  # ChromaDB
lsof -i :19530 # Milvus

# Stop conflicting services or change ports in docker-compose.yml
```

### Database Not Ready

If tests fail with connection errors:

```bash
# Check service health
docker-compose ps

# View logs for a specific service
docker-compose logs qdrant
docker-compose logs postgres
docker-compose logs chromadb
docker-compose logs milvus

# Restart services if needed
docker-compose restart
```

### Out of Memory

If Docker runs out of memory:

```bash
# Check Docker memory usage
docker stats

# Increase Docker's memory limit in Docker Desktop settings
# Or reduce the number of running services in docker-compose.yml
```

### Clean State for Re-running Tests

```bash
# Complete cleanup and restart
docker-compose down -v
docker-compose up -d
sleep 15
cargo test --test integration_test -- --ignored --test-threads=1
```

## CI/CD Integration

For automated testing in CI pipelines:

```yaml
# Example GitHub Actions workflow
name: Integration Tests

on: [push, pull_request]

jobs:
  integration:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Start databases
        run: docker-compose up -d
        working-directory: crates/oxify-connect-vector

      - name: Wait for databases
        run: sleep 30

      - name: Run integration tests
        run: cargo test --test integration_test -- --ignored --test-threads=1
        working-directory: crates/oxify-connect-vector

      - name: Cleanup
        if: always()
        run: docker-compose down -v
        working-directory: crates/oxify-connect-vector
```

## Test Structure

### Helper Module (`integration_helpers.rs`)

Provides utilities:
- **URL getters**: Get database URLs from environment or defaults
- **Collection name generator**: Create unique collection names to avoid conflicts
- **Test data generators**: Generate test vectors and documents
- **Vector operations**: Normalize vectors, etc.

### Test File (`integration_test.rs`)

Each test follows this pattern:

1. Create provider instance
2. Create unique collection
3. Verify collection exists
4. Insert test vectors
5. Perform searches
6. Test batch operations
7. Test updates/deletes
8. Verify results

## Writing New Integration Tests

Template for new provider tests:

```rust
#[tokio::test]
#[ignore]
async fn test_new_provider_integration() {
    let provider = NewProvider::new(&new_provider_url());
    let collection = unique_collection_name("new_test");

    // Create collection
    provider.create_collection(&collection, TEST_DIMENSION)
        .await
        .expect("Failed to create collection");

    // Your test logic here...

    println!("✓ New provider integration test passed");
}
```

## Performance Considerations

- Integration tests are slower than unit tests (network I/O, actual database operations)
- Run tests sequentially (`--test-threads=1`) to avoid resource contention
- Each test creates a unique collection to avoid conflicts
- Cleanup is automatic (collections are not shared between tests)

## Security Notes

- Test databases use default credentials (`test/test`) for simplicity
- **Never use these configurations in production**
- Test databases are exposed on localhost only
- All data is ephemeral (cleared with `docker-compose down -v`)

## Support

If you encounter issues:

1. Check the [Troubleshooting](#troubleshooting) section
2. Verify Docker and Docker Compose versions
3. Check database logs: `docker-compose logs [service]`
4. Ensure sufficient system resources (RAM, disk space)
5. Open an issue with full error logs and environment details
