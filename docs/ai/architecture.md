# Forge Architecture

**Last updated**: 2026-02-16

## Overview

Forge is a service-oriented code forge and IPS packaging platform. The `forged` server stores source information for IPS packages and coordinates builds via Solstice CI.

## System Components

```
┌──────────────────────────────────────────────────┐
│  Transport Layer (gRPC/tonic + tower auth)       │
│  AuthService, GateService, ComponentService      │
│  BuildService (+dispatch), GitService (exp.)     │
└──────────────────┬───────────────────────────────┘
                   │
┌──────────────────▼───────────────────────────────┐
│  Service Layer (Business Logic + RBAC)           │
│  AuthService, GateManager, ComponentManager      │
│  RbacService, OidcService, BuildDispatch         │
│  BuildReportConsumer, BlobService                │
└──────────────────┬───────────────────────────────┘
                   │
┌──────────────────▼───────────────────────────────┐
│  Repository Layer (Data Access / SeaORM)         │
│  GateRepo, ComponentRepo, BlobRepo               │
│  SourceArchiveRepo, ActorRepo, BuildJobRepo      │
└──────────────────┬───────────────────────────────┘
                   │
┌──────────────────▼───────────────────────────────┐
│  Storage / Messaging Layer                       │
│  PostgreSQL │ SeaweedFS │ Jujutsu │ RabbitMQ     │
└──────────────────────────────────────────────────┘
```

## Dependency Graph (AppState)

```
AppState::new()
 ├─ Database (PostgreSQL + auto-migrations)
 ├─ SeaweedFsClient
 ├─ JjRepoManager
 ├─ AMQP Pool (deadpool-lapin → RabbitMQ)
 ├─ Repositories
 │    ├─ BlobRepository (db, seaweedfs)
 │    ├─ SourceArchiveRepository (db, blob_repo)
 │    ├─ GateRepository (db, jj_manager)
 │    ├─ ComponentRepository (db, blob_repo, jj_manager)
 │    ├─ ActorRepository (db)
 │    └─ BuildJobRepository (db)
 └─ Services
      ├─ OidcService (OIDC discovery + JWT validation)
      ├─ AuthService (OIDC auth, SSH key registration, confirmation)
      ├─ RbacService (gate_repo, component_repo)
      ├─ GateManager (gate_repo, component_repo, rbac)
      ├─ ComponentManager (component_repo, source_archive_repo, rbac)
      ├─ BlobService (db, blob_repo, rbac)
      └─ BuildDispatchService (build_job_repo, rbac, amqp_pool)
```

## Authentication & Authorization

- **OIDC**: Bearer tokens validated via OidcService (JWKS discovery, JWT signature/claims verification). Tokens issued by external OIDC provider, not forged.
- **SSH key registration**: Age-encrypted challenge/response flow for CLI users without OIDC.
- **Auth middleware**: Tower layer extracts Bearer token, validates via OIDC, injects `AuthenticatedActor` into request extensions.
- **RBAC**: Role-based (Admin/Member/Viewer) + permission-based (GateAdmin/GateRead/GateWrite/ComponentRead/ComponentWrite). Enforced at both service and transport layers.

## KDL Validation

Gate and component KDL is validated before storage using `knuffel::parse::<gate::Gate>()` and `knuffel::parse::<component::Recipe>()` respectively. Parse errors are returned as miette diagnostics with example syntax.

## Build Dispatch (Solstice CI Integration)

Forged dispatches builds to Solstice CI via RabbitMQ:

1. **Submit**: `BuildDispatchService.submit_build()` → creates `build_job` record → publishes `JobRequest` (JSON, wire-compatible with solstice-ci `common` crate) to AMQP exchange
2. **Track**: `build_job` table tracks status (queued/running/success/failed/cancelled)
3. **Consume**: `BuildReportConsumer` listens on results queue, updates `build_job` on `JobResult`
4. **Topology**: Direct exchanges with durable queues, matching existing worker/ghwhrecv patterns

Note: Solstice-ci `common` crate cannot be used as a git dependency (SHA256 object format, cargo#14942). Compatible message types are re-implemented in `services/build_dispatch.rs`.

## Storage Backends

- **PostgreSQL**: Metadata for actors, gates, components, blobs, operations, memberships, build jobs. Accessed exclusively through SeaORM entities.
- **SeaweedFS**: Content-addressable blob storage (source archives, patches, licenses, scripts). SHA256 hashing with deduplication.
- **Jujutsu**: Custom `SeaweedFsBackend` (`storage/jj_backend/`) that stores commits/trees in PostgreSQL and file contents in SeaweedFS. Each gate and component gets its own Jujutsu repository.
- **RabbitMQ**: AMQP messaging for build dispatch and result reporting. Uses deadpool-lapin for connection pooling.

## Completion Status

### Complete
- All gRPC services with all RPCs implemented (auth, gate, component, build)
- Real OIDC authentication with JWT validation
- SSH key registration/confirmation flow (age-encrypted challenges)
- Tower auth middleware with actor injection
- RBAC enforcement at transport and service layers
- KDL validation for gates and components
- Build dispatch via RabbitMQ (submit, status, list, cancel)
- Background build report consumer
- All repositories with full CRUD (including build_job)
- PostgreSQL, SeaweedFS, and Jujutsu storage integration
- Streaming upload/download for large files
- Test infrastructure (TestContext with isolated databases)

### Known Issues
- **jj-lib 0.24 incompatibility**: ~370 Send/Sync errors from jj-lib's `WorkingCopy` trait not being Send/Sync. Affects all async methods in types that transitively hold `JjRepoManager`. Requires jj-lib upgrade or architecture change.
- **QUIC transport**: Placeholder only
- **`list_accessible_gates()`** in RbacService returns empty vec
- **Jujutsu sync** (`jj_repos/sync.rs`): Implementation pending

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| forged | Main gRPC server |
| forged-client | gRPC client + proto definitions (gate, component, auth, git — not api_v2) |
| forged-migration | SeaORM migrations (auto-run on startup) |
| pkgdev | CLI for package development (auth, gate/component ops, build, metadata) |
| component | KDL domain model for package recipes (knuffel parser) |
| gate | KDL domain model for gates |
| worker | RabbitMQ background job processor |
| xtask | Build automation (illumos sysroot generation) |
| github / ghwhrecv | GitHub webhook receiver |
| forge_config / integration / repology / workspace | Supporting crates |

## Proto Files

Located at `crates/forged/proto/`. Adding a new proto requires updating:
1. The proto file itself
2. `crates/forged/build.rs` (server stubs, includes serde derives)
3. `crates/forged-client/build.rs` (client stubs, no serde) — note: `api_v2.proto` is server-only
