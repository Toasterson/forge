# Forged V2 - Implementation Complete

## Overview

This is the completed V2 rewrite of the forged daemon using:
- **PostgreSQL** for metadata
- **SeaweedFS** for content-addressed blob storage
- **Jujutsu** for version control
- **gRPC** with Protocol Buffers for client-server communication

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     gRPC API v2                             │
│  (AuthService, GateService, ComponentService, BuildService) │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                   Business Logic Layer                      │
│  GateManager │ ComponentManager │ BlobService │ RbacService │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    Repository Layer                         │
│  GateRepo │ ComponentRepo │ BlobRepo │ SourceArchiveRepo    │
└─────────────────────────────────────────────────────────────┘
                              │
┌──────────────────┬──────────────────┬──────────────────────┐
│   PostgreSQL     │    SeaweedFS     │      Jujutsu         │
│   (Metadata)     │  (Blob Storage)  │  (Version Control)   │
└──────────────────┴──────────────────┴──────────────────────┘
```

## What Was Implemented

### Phase 1: Database Foundation ✅
- **Migration System** (`migration/src/`)
  - 8 tables: `blob_metadata`, `gates`, `gate_members`, `components`, `component_files`, `source_archives`, `actors`, `operations`
  - Foreign key relationships with cascading deletes
  - Proper indexing for performance

- **SeaORM Entities** (`src/entities/`)
  - All 8 entities with relationships
  - Custom types (ActorKind, BlobType, FileKind, Permission, Role)

### Phase 2: Repository Layer ✅
- **BlobRepository** - Content-addressed storage with PostgreSQL + SeaweedFS integration
- **SourceArchiveRepository** - Source archive management
- **ActorRepository** - User/service registry with OIDC support
- **GateRepository** - Gate management with Jujutsu integration
- **ComponentRepository** - Component lifecycle (orchestrates all 3 storage layers)

**Key Features:**
- Content-addressed deduplication
- Jujutsu commits for all entity changes
- Miette diagnostic pattern for user-friendly errors

### Phase 3: gRPC API v2 ✅
- **Protocol Buffers** (`proto/api_v2.proto`)
  - 4 services: Auth, Gate, Component, Build
  - Streaming support for large file uploads/downloads

- **Service Implementations** (`src/transport/grpc/`)
  - `AuthService` - OIDC token validation
  - `GateService` - Gate CRUD + member management
  - `ComponentService` - Component CRUD + file uploads (streaming)
  - `BuildService` - Build manifest + blob downloads (streaming, 64KB chunks)

### Phase 4: Business Logic ✅
- **OidcService** - Token validation (stub for MVP, documented for production)
- **RbacService** - Permission checking with gate inheritance
- **GateManager** - High-level gate operations with RBAC
- **ComponentManager** - Component orchestration with permission checks
- **BlobService** - Access-controlled blob downloads

**RBAC Model:**
- Gate owner has all permissions
- Members have role-based permissions
- Component permissions inherit from gate permissions

### Phase 5: Configuration & Integration ✅
- **Settings** (`src/settings.rs`) - PostgreSQL, SeaweedFS, Jujutsu, OIDC config
- **AppState** (`src/app_state.rs`) - Dependency injection for all layers
- **main.rs** - Server initialization with graceful shutdown
- **docker-compose.yml** - Full development stack
- **Dockerfile** - Multi-stage build with Rust 1.85
- **Integration Tests** (`tests/integration_test.rs`)

## Quick Start

### Development with Docker Compose

1. **Start infrastructure:**
```bash
cd crates/forged
docker-compose up -d postgres seaweedfs-master seaweedfs-volume
```

2. **Run migrations:**
```bash
cargo run -p forged-migration
```

3. **Start server:**
```bash
FORGED__POSTGRES__URL="postgresql://forged:forged@localhost/forged" \
FORGED__SEAWEEDFS__MASTER_URL="http://localhost:9333" \
cargo run -p forged
```

### Full Stack with Docker

```bash
docker-compose up --build
```

Server will be available at `localhost:50051`

## Configuration

### Environment Variables

```bash
# Server
FORGED__SERVER__LISTEN_ADDR="0.0.0.0:50051"

# PostgreSQL (required)
FORGED__POSTGRES__URL="postgresql://user:password@host/database"
FORGED__POSTGRES__MAX_CONNECTIONS="20"

# SeaweedFS (required)
FORGED__SEAWEEDFS__MASTER_URL="http://localhost:9333"
FORGED__SEAWEEDFS__NAMESPACE="default"

# Jujutsu Repos
FORGED__JJ_REPOS__ROOT="./data/jj-repos"

# OIDC (optional for MVP)
FORGED__OIDC__ISSUER_URL="https://auth.example.com"
FORGED__OIDC__CLIENT_ID="forged-client"
FORGED__OIDC__AUDIENCE="forged-api"
```

### Configuration File

Create `forged.toml`:
```toml
[server]
listen_addr = "0.0.0.0:50051"

[postgres]
url = "postgresql://forged:forged@localhost/forged"
max_connections = 20

[seaweedfs]
master_url = "http://localhost:9333"
namespace = "default"

[jj_repos]
root = "./data/jj-repos"

[oidc]
issuer_url = "https://auth.example.com"
client_id = "forged-client"
audience = "forged-api"
```

## Client Workflow

### 1. Authenticate
```protobuf
AuthService.Authenticate(oidc_token) → ActorRef
```

### 2. Get Build Manifest
```protobuf
BuildService.GetBuildManifest(component_id) → BuildManifest
```

Returns:
- Recipe KDL
- List of source archives (hash, size, filename)
- List of patches, licenses, scripts (hash, size, rel_path)

### 3. Download Files
```protobuf
BuildService.DownloadBlob(hash, blob_type) → stream bytes
```

Files are streamed in 64KB chunks.

### 4. Build Component
Client follows recipe KDL to build packages.

### 5. Upload Packages
Client uploads directly to IPS depot (forged not involved).

## Testing

### Run Integration Tests

```bash
# Start dependencies
docker-compose up -d postgres seaweedfs-master seaweedfs-volume

# Run tests
cargo test --test integration_test -- --ignored --test-threads=1
```

Tests cover:
- Database connection and migrations
- Actor creation from OIDC
- Gate lifecycle with Jujutsu integration
- Component creation with file uploads
- RBAC permission enforcement

## Database Schema

```sql
actors (id, kind, oidc_sub, display_name, created_at, updated_at)
  └─> gates (owner_id)
  └─> gate_members (actor_id)
  └─> operations (actor_id)

gates (id, name, gate_kdl, owner_id, created_at, updated_at)
  └─> gate_members (gate_id)
  └─> components (gate_id)

components (id, gate_id, name, recipe_kdl, created_at, updated_at)
  └─> source_archives (component_id)
  └─> component_files (component_id)

blob_metadata (id, hash, blob_type, namespace, fid, size_bytes, created_at)
```

## File Structure

```
forged/
├── migration/               # Database migrations
│   └── src/
│       ├── lib.rs
│       ├── main.rs
│       └── m20260120_000001_create_initial_schema.rs
├── proto/                   # Protocol Buffers
│   └── api_v2.proto
├── src/
│   ├── entities/            # SeaORM entities
│   │   ├── actor.rs
│   │   ├── blob_metadata.rs
│   │   ├── component.rs
│   │   ├── component_file.rs
│   │   ├── gate.rs
│   │   ├── gate_member.rs
│   │   ├── operation.rs
│   │   └── source_archive.rs
│   ├── repositories/        # Data access layer
│   │   ├── actor_repository.rs
│   │   ├── blob_repository.rs
│   │   ├── component_repository.rs
│   │   ├── gate_repository.rs
│   │   └── source_archive_repository.rs
│   ├── services/            # Business logic
│   │   ├── blob_service.rs
│   │   ├── component_manager.rs
│   │   ├── gate_manager.rs
│   │   ├── oidc_service.rs
│   │   └── rbac_service.rs
│   ├── transport/grpc/      # gRPC services
│   │   ├── auth_service.rs
│   │   ├── build_service.rs
│   │   ├── component_service.rs
│   │   └── gate_service.rs
│   ├── app_state.rs         # Dependency injection
│   ├── settings.rs          # Configuration
│   └── main.rs              # Server entry point
├── tests/
│   └── integration_test.rs  # Integration tests
├── docker-compose.yml       # Development stack
└── Dockerfile               # Multi-stage build
```

## Deferred Features (Post-MVP)

- **Multi-replica sync** - Operation log synchronization (Phase 7 from plan)
- **V1 migration tool** - Fresh start for MVP
- **Garbage collection** - Manual cleanup acceptable for now
- **QUIC transport** - gRPC sufficient
- **Production OIDC** - Stub implementation for MVP

## Next Steps

### To Complete MVP:
1. ✅ All database tables and migrations
2. ✅ All repositories with Jujutsu integration
3. ✅ All gRPC services with streaming
4. ✅ All business logic services
5. ✅ Docker Compose stack
6. ⏳ **Test with real client** - forged-client integration
7. ⏳ **Implement real OIDC** - Replace stub in OidcService
8. ⏳ **Performance testing** - Large file uploads/downloads
9. ⏳ **Documentation** - API reference, client guide

### Production Readiness:
- Real OIDC integration (commented in `oidc_service.rs`)
- Production database with connection pooling
- SeaweedFS replication configuration
- Kubernetes deployment manifests
- Metrics and monitoring (Prometheus/Grafana)
- Audit logging
- Rate limiting

## Troubleshooting

### Database Connection Failed
```
Error: Failed to connect to PostgreSQL

Solution:
1. Verify PostgreSQL is running: docker-compose ps postgres
2. Check connection string: FORGED__POSTGRES__URL
3. Test connection: psql postgresql://forged:forged@localhost/forged
```

### SeaweedFS Upload Failed
```
Error: failed to upload blob to SeaweedFS volume server

Solution:
1. Verify SeaweedFS is running: docker-compose ps seaweedfs-*
2. Check master status: curl http://localhost:9333/cluster/status
3. Verify volume server: curl http://localhost:8080/status
```

### Migration Errors
```
Error: failed to run database migrations

Solution:
1. Drop and recreate database: dropdb forged_test && createdb forged_test
2. Run migrations manually: cargo run -p forged-migration
3. Check migration logs for specific errors
```

## License

See workspace LICENSE file.

## Contributing

This is part of the illumos/forge project. See main repository for contribution guidelines.
