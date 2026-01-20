# Forged Integration Tests

Comprehensive integration tests for Forged V2 using cargo nextest.

## Test Organization

```
tests/
├── common/              # Test infrastructure
│   ├── mod.rs          # TestContext with isolated databases
│   └── fixtures.rs     # Test data builders
├── repository/         # Repository layer tests
│   ├── blob_repository_test.rs
│   ├── gate_repository_test.rs
│   └── component_repository_test.rs
├── service/            # Service layer tests
│   ├── rbac_service_test.rs
│   └── component_manager_test.rs
└── integration/        # Integration tests
    ├── e2e_workflow_test.rs
    ├── concurrent_operations_test.rs
    └── streaming_test.rs
```

## Prerequisites

1. **Install cargo-nextest:**
   ```bash
   cargo install cargo-nextest
   ```

2. **Start test infrastructure:**
   ```bash
   docker-compose -f docker-compose.test.yml up -d
   ```

   This starts:
   - PostgreSQL 17 on port 5433
   - SeaweedFS master on port 9334
   - SeaweedFS volume on port 8081

## Running Tests

### Quick Start

Use the provided test runner script:

```bash
./scripts/run_tests.sh
```

This will:
1. Start Docker Compose services
2. Run migrations
3. Execute all tests with nextest
4. Clean up infrastructure

### Manual Execution

1. **Start infrastructure:**
   ```bash
   docker-compose -f docker-compose.test.yml up -d
   ```

2. **Set environment variables:**
   ```bash
   export TEST_DATABASE_URL="postgresql://forged:forged@localhost:5433/forged_test"
   export TEST_SEAWEEDFS_URL="http://localhost:9334"
   ```

3. **Run migrations:**
   ```bash
   cargo run -p forged-migration
   ```

4. **Run tests:**
   ```bash
   cargo nextest run --all-features
   ```

5. **Cleanup:**
   ```bash
   docker-compose -f docker-compose.test.yml down -v
   ```

### Test Profiles

Nextest supports multiple profiles (see `.config/nextest.toml`):

- **default**: Standard local testing
  ```bash
  cargo nextest run
  ```

- **ci**: CI-optimized with retries
  ```bash
  cargo nextest run --profile ci
  ```

- **integration**: Sequential execution for database safety
  ```bash
  cargo nextest run --profile integration
  ```

### Running Specific Tests

```bash
# Run all tests in a specific file
cargo nextest run --test blob_repository_test

# Run tests matching a pattern
cargo nextest run blob

# Run a specific test
cargo nextest run test_blob_storage_and_retrieval

# Run tests in a directory
cargo nextest run --test-threads 1 integration::
```

### Including Ignored Tests

Some tests (like large file uploads) are marked `#[ignore]` for performance:

```bash
cargo nextest run --run-ignored all
```

## Test Infrastructure

### TestContext

Each test gets an isolated environment:

- **Unique database**: `forged_test_<uuid>` created per test
- **Unique namespace**: SeaweedFS namespace per test
- **Unique Jujutsu repos**: `./test_data/<uuid>/jj-repos`
- **Automatic cleanup**: Database and files removed after test

Example:

```rust
use common::TestContext;

#[tokio::test]
async fn my_test() {
    let ctx = TestContext::new().await;
    // Use ctx.app_state for all operations
}
```

### Test Fixtures

Convenient builders for common test data:

```rust
use common::fixtures::TestFixtures;

let owner = TestFixtures::actor(&ctx, "owner").await;
let gate = TestFixtures::gate(&ctx, &owner, "my-gate").await;
let component = TestFixtures::component(&ctx, &owner, &gate, "my-component").await;
let blob_hash = TestFixtures::blob(&ctx, b"data").await;
```

## Test Categories

### Repository Layer Tests

Test data access and storage:

- Blob storage and retrieval
- Deduplication
- Gate and component lifecycle
- Jujutsu integration

### Service Layer Tests

Test business logic and RBAC:

- Permission checks
- Member management
- Component operations
- Manifest aggregation

### Integration Tests

Test complete workflows:

- **E2E workflow**: Full component creation and build manifest generation
- **Concurrent operations**: Parallel uploads, creations, and reads
- **Streaming**: Large file handling (1MB to 100MB)

## Continuous Integration

Tests run automatically on:

- Push to `main` or `develop`
- Pull requests to `main` or `develop`

See `.github/workflows/test.yml` for CI configuration.

## Troubleshooting

### Tests Hang

If tests hang, check if services are healthy:

```bash
docker-compose -f docker-compose.test.yml ps
```

### Database Connection Errors

Ensure PostgreSQL is accessible:

```bash
psql postgresql://forged:forged@localhost:5433/forged_test -c "SELECT 1"
```

### SeaweedFS Errors

Check SeaweedFS status:

```bash
curl http://localhost:9334/cluster/status
curl http://localhost:8081/status
```

### Orphaned Test Databases

If tests crash without cleanup, manually drop databases:

```bash
psql postgresql://forged:forged@localhost:5433/postgres -c "
  SELECT 'DROP DATABASE ' || datname || ';'
  FROM pg_database
  WHERE datname LIKE 'forged_test_%'
"
```

### Cleanup Test Data

Remove orphaned Jujutsu repos:

```bash
rm -rf ./test_data/forged_test_*
```

## Writing New Tests

1. **Add test file** in appropriate directory (repository/service/integration)
2. **Import common module**: `mod common;`
3. **Use TestContext** for isolation: `let ctx = TestContext::new().await;`
4. **Use TestFixtures** for test data
5. **Mark long-running tests** with `#[ignore]`

Example:

```rust
mod common;
use common::{fixtures::TestFixtures, TestContext};

#[tokio::test]
async fn test_my_feature() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    // ... test implementation
}
```

## Performance

- **Test suite**: ~5 minutes (with nextest parallelization)
- **Individual test**: <30 seconds
- **Large file tests**: <2 minutes (ignored by default)
