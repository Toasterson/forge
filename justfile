# Forge development commands

# Default: list available recipes
default:
    @just --list

# Start all development services (PostgreSQL, SeaweedFS, RabbitMQ)
services-up:
    docker compose -f docker-compose.dev.yml up -d
    @echo "Waiting for services to be ready..."
    @just _wait-for-postgres
    @just _wait-for-seaweedfs
    @just _wait-for-rabbitmq
    @just _ensure-test-db
    @echo "All services ready."

# Stop all development services
services-down:
    docker compose -f docker-compose.dev.yml down

# Stop services and remove volumes (clean slate)
services-clean:
    docker compose -f docker-compose.dev.yml down -v

# Show service status
services-status:
    @docker compose -f docker-compose.dev.yml ps

# Build the entire workspace
build:
    cargo build --workspace

# Run lints (fmt check + clippy)
lint:
    cargo fmt --all -- --check
    cargo clippy --workspace -- -D warnings

# Run unit tests (no external services needed)
test-unit:
    cargo test -p forged --lib
    cargo test -p component
    cargo test -p gate

# Run integration tests (requires services)
test-integration: services-up
    cargo test -p forged --test integration_test -- --ignored --nocapture

# Run e2e tests (requires services)
test-e2e: services-up
    cargo test -p forged --test e2e_git -- --ignored --nocapture

# Run all tests (unit + integration + e2e)
test-all: services-up
    cargo test --workspace
    cargo test -p forged --test integration_test -- --ignored --nocapture
    cargo test -p forged --test e2e_git -- --ignored --nocapture

# Run e2e tests with nextest
test-e2e-nextest: services-up
    cd crates/forged && cargo nextest run --profile integration -E 'test(e2e_)' -- --ignored

# Build the mdbook documentation
docs:
    cd docs/book && mdbook build

# Serve the mdbook documentation locally
docs-serve:
    cd docs/book && mdbook serve --open

# Run the forged server (development mode)
run:
    RUST_LOG=forged=debug cargo run -p forged

# Run database migrations
migrate:
    cargo run -p forged-migration

# --- Internal helpers ---

_wait-for-postgres:
    #!/usr/bin/env bash
    set -e
    for i in $(seq 1 30); do
        if docker compose -f docker-compose.dev.yml exec -T postgres pg_isready -U forged >/dev/null 2>&1; then
            echo "  PostgreSQL is ready."
            exit 0
        fi
        sleep 1
    done
    echo "ERROR: PostgreSQL did not become ready in 30 seconds"
    exit 1

_wait-for-seaweedfs:
    #!/usr/bin/env bash
    set -e
    for i in $(seq 1 30); do
        if curl -sf http://localhost:9333/cluster/status >/dev/null 2>&1; then
            echo "  SeaweedFS is ready."
            exit 0
        fi
        sleep 1
    done
    echo "ERROR: SeaweedFS did not become ready in 30 seconds"
    exit 1

_wait-for-rabbitmq:
    #!/usr/bin/env bash
    set -e
    for i in $(seq 1 30); do
        if docker compose -f docker-compose.dev.yml exec -T rabbitmq rabbitmqctl status >/dev/null 2>&1; then
            echo "  RabbitMQ is ready."
            exit 0
        fi
        sleep 1
    done
    echo "ERROR: RabbitMQ did not become ready in 30 seconds"
    exit 1

_ensure-test-db:
    #!/usr/bin/env bash
    set -e
    # Create the forged_test database if it doesn't exist (used as base for per-test isolated DBs)
    docker compose -f docker-compose.dev.yml exec -T postgres \
        psql -U forged -d postgres -tc "SELECT 1 FROM pg_database WHERE datname = 'forged_test'" \
        | grep -q 1 || \
    docker compose -f docker-compose.dev.yml exec -T postgres \
        psql -U forged -d postgres -c "CREATE DATABASE forged_test OWNER forged"
    echo "  Test database ready."
