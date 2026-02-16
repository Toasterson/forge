# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Documentation Maintenance

- Keep `docs/ai/architecture.md` updated when making structural changes (new crates, protocol changes, phase transitions). Bump the "last updated" date.
- Create a new timestamped plan in `docs/ai/plans/` before starting a new phase or significant feature.
- Create a new timestamped ADR in `docs/ai/decisions/` when making meaningful technology or design choices. Number sequentially from the last ADR.
- Never delete old plans or decisions. Mark superseded plans with status `Superseded` and link to the replacement.

## Project Overview

Forge is an experimental, service-oriented code forge and packaging platform built in Rust. It targets illumos/Solaris packaging workflows, uses gRPC for transport, PostgreSQL + SeaORM for metadata, SeaweedFS for blob storage, and Jujutsu (jj-lib) as the VCS backend. The main server binary is `forged`.

**Status**: Pre-alpha. License: MPL-2.0.

## Build & Development Commands

**Prerequisites**: Rust stable, protobuf-compiler, clang, pkg-config, libssl-dev, libarchive-dev

```bash
# Build everything
cargo build --workspace

# Build and run the server
cargo run -p forged

# Lint
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# Run all tests (unit + integration, e2e ignored by default)
cargo test --workspace

# Run tests for a specific crate
cargo test -p forged
cargo test -p component

# Run a single test by name
cargo test -p forged -- test_name

# Run tests with nextest (used in CI, config at crates/forged/.config/nextest.toml)
cd crates/forged && cargo nextest run --profile ci --all-features

# Run e2e tests (normally ignored, requires running services)
cargo test -p forged --test e2e_git -- --ignored --nocapture

# Run database migrations
cargo run -p forged-migration

# Cross-compile for illumos
cross build --target x86_64-unknown-illumos -p forged -p pkgdev --release
```

## Test Infrastructure

Tests require PostgreSQL and SeaweedFS. Start them via:
```bash
docker compose -f docker-compose.dev.yml up -d
```

Key env vars for tests:
- `TEST_DATABASE_URL` (default: `postgresql://forged:forged@localhost/forged_test`)
- `TEST_SEAWEEDFS_URL` (default: `http://localhost:9333`)

`TestContext` (`crates/forged/tests/common/mod.rs`) creates an isolated PostgreSQL database per test (auto-cleaned on drop). `TestFixtures` (`tests/common/fixtures.rs`) provides builder-pattern test data. Tests are organized under `crates/forged/tests/` into `repository/`, `service/`, `grpc/`, and `integration/` directories matching the architectural layers.

Nextest profiles: `default` (4 threads), `ci` (2 threads, retries), `integration` (sequential, long timeouts).

## Architecture

The `forged` server (`crates/forged/`) follows a layered architecture:

```
transport/  → gRPC service handlers (tonic). Protobuf definitions in proto/
services/   → Business logic, RBAC enforcement
repositories/ → Data access layer (SeaORM queries)
storage/    → Backend implementations (PostgreSQL, SeaweedFS, Jujutsu)
entities/   → SeaORM entity models (actor, blob_metadata, component, gate, etc.)
```

Shared state flows through `AppState` (`app_state.rs`) which holds DB connections, storage clients, and service instances behind `Arc`.

### Workspace Crates

- **forged** — Main gRPC server binary with all layers above
- **forged-client** — Shared gRPC client + protobuf definitions
- **forged-migration** — SeaORM database migrations
- **pkgdev** — CLI tool for package development (KDL metadata, gRPC client to forged)
- **component** / **gate** — Core domain models parsed from KDL (`package.kdl`, gate definitions)
- **github** / **ghwhrecv** — GitHub webhook receiver
- **xtask** — Build automation (illumos sysroot generation via libips)
- **worker** — Background worker (WIP)
- **forge_config** / **integration** / **repology** / **workspace** — Supporting crates

### gRPC Services (proto/)

- `AuthService` — Public key authentication (Ed25519), registration, JWT token issuance
- `GateService` — Gate (repository group) CRUD, member management
- `ComponentService` — Component CRUD, file management, source archives
- `BuildService` — Build manifest generation, blob streaming
- `GitService` — Git protocol operations (experimental)

### Configuration

Loaded by `Settings::load()` in `settings.rs`: `FORGED_CONFIG` env → `./forged.toml` → environment overrides (`FORGED__*` with `__` separator) → defaults. Sections: `server`, `postgres`, `seaweedfs`, `jj_repos`, `oidc`.

## Key Conventions

- **Database access**: Always use SeaORM entities and query builders, never raw SQL
- **Error handling**: Use `miette` for user-facing diagnostics with actionable help text. Use `thiserror` for library error types
- **Package metadata**: Defined in KDL format (`package.kdl`), parsed by the `component` crate using the `knuffel` parser
- **Async runtime**: Tokio with full features. Shared state uses `Arc`
- **Feature flags**: `otel` (OpenTelemetry export), `quic` (QUIC transport endpoint) on the `forged` crate
