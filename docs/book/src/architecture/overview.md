# Architecture Overview

Forge follows a layered architecture with clear separation between transport, business logic, data access, and storage.

## Layers

```
┌─────────────────────────────────────────┐
│           Transport (gRPC/tonic)        │
│  Auth middleware, request/response ser. │
├─────────────────────────────────────────┤
│           Services (business logic)     │
│  AuthService, GateManager, RBAC, etc.  │
├─────────────────────────────────────────┤
│         Repositories (data access)      │
│  SeaORM queries, entity mapping         │
├─────────────────────────────────────────┤
│            Storage (backends)           │
│  PostgreSQL, SeaweedFS, Jujutsu, AMQP  │
└─────────────────────────────────────────┘
```

### Transport Layer

gRPC service handlers built with [tonic](https://github.com/hyperium/tonic). Protobuf definitions live in `crates/forged/proto/`. A Tower-based auth middleware intercepts requests for authentication and authorization.

### Service Layer

Business logic including:

- **AuthService / OidcService** -- Actor registration, OIDC token validation, key management
- **GateManager** -- Gate CRUD and member management
- **ComponentManager** -- Component CRUD, file and archive management
- **RbacService** -- Role-based access control enforcement
- **BlobService** -- Content-addressable blob operations
- **BuildDispatchService** -- Build job submission to Solstice CI via AMQP
- **BuildReportConsumer** -- Listens for build results on RabbitMQ

### Repository Layer

Data access using [SeaORM](https://www.sea-ql.org/SeaORM/) with PostgreSQL. Each domain entity has a corresponding repository struct providing query methods. Raw SQL is never used.

### Storage Layer

- **PostgreSQL** -- Metadata storage (actors, gates, components, build jobs)
- **SeaweedFS** -- Content-addressable blob storage for source archives, patches, and build artifacts (SHA-256 keyed)
- **Jujutsu** -- Version control backend with a custom SeaweedFS storage adapter
- **RabbitMQ** -- Message queue for build dispatch and result consumption

## Shared State

All layers are wired together through `AppState` (`crates/forged/src/app_state.rs`), which holds database connections, storage clients, and service instances behind `Arc`. It is passed into each gRPC handler.

## Workspace Crates

| Crate | Purpose |
|---|---|
| `forged` | Main gRPC server binary |
| `forged-client` | Shared gRPC client and protobuf definitions |
| `forged-migration` | SeaORM database migrations |
| `pkgdev` | CLI tool for package developers |
| `component` | KDL component recipe parser |
| `gate` | KDL gate definition parser |
| `github` / `ghwhrecv` | GitHub webhook receiver |
| `worker` | Background job processor (WIP) |
| `xtask` | Build automation (illumos sysroot generation) |
| `forge_config` | Configuration types |
| `integration` | Integration test utilities |
