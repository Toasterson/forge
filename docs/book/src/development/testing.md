# Testing

## Prerequisites

Tests require PostgreSQL and SeaweedFS. Start them with Docker Compose:

```bash
docker compose -f docker-compose.dev.yml up -d
```

## Running Tests

```bash
# Run all tests (unit + integration)
cargo test --workspace

# Run tests for a specific crate
cargo test -p forged
cargo test -p component
cargo test -p gate

# Run a single test by name
cargo test -p forged -- test_name

# Run with nextest (used in CI)
cd crates/forged && cargo nextest run --profile ci --all-features
```

## End-to-End Tests

E2E tests are ignored by default because they require running services:

```bash
cargo test -p forged --test e2e_git -- --ignored --nocapture
```

## Test Infrastructure

### TestContext

`TestContext` (in `crates/forged/tests/common/mod.rs`) creates an isolated PostgreSQL database for each test. The database is automatically cleaned up when the `TestContext` is dropped.

### TestFixtures

`TestFixtures` (in `crates/forged/tests/common/fixtures.rs`) provides builder-pattern helpers for creating test data.

## Test Organization

Tests are organized under `crates/forged/tests/` by architectural layer:

```
tests/
  repository/   -- Data access layer tests
  service/      -- Business logic tests
  grpc/         -- Transport layer tests
  integration/  -- Cross-layer integration tests
```

## Nextest Profiles

Configured in `crates/forged/.config/nextest.toml`:

| Profile | Threads | Retries | Use |
|---|---|---|---|
| `default` | 4 | 0 | Local development |
| `ci` | 2 | 2 | CI pipelines |
| `integration` | 1 (sequential) | 0 | Integration tests with long timeouts |

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `TEST_DATABASE_URL` | `postgresql://forged:forged@localhost/forged_test` | Test database connection |
| `TEST_SEAWEEDFS_URL` | `http://localhost:9333` | Test SeaweedFS instance |
