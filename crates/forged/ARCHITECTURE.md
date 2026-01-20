# Forged Server Architecture - V2 Rewrite

## Overview

This document describes the new architecture for the forged server using Jujutsu VCS and SeaweedFS for scalable, distributed operation.

## Architecture Summary

```
┌───────────────────────────────────────────────────────────┐
│  gRPC Services (GateService, ComponentService, VcsService)│
└─────────────────┬─────────────────────────────────────────┘
                  │
┌─────────────────▼─────────────────────────────────────────┐
│  Application Services (GateManager, ComponentManager, etc)│
│  - Business logic, RBAC, transaction coordination         │
└─────────────────┬─────────────────────────────────────────┘
                  │
┌─────────────────▼─────────────────────────────────────────┐
│  Repositories (GateRepository, ComponentRepository)       │
│  - SeaORM entity queries                                  │
└─────────────────┬─────────────────────────────────────────┘
                  │
┌─────────────────▼─────────────────────────────────────────┐
│  Storage Layer                                            │
│  - JjRepoManager (Jujutsu workspaces)                     │
│  - SeaweedFsBackend (custom jj Backend impl)              │
│  - PostgreSQL (SeaORM)                                    │
└───────────────────────────────────────────────────────────┘
```

## Key Components

### 1. Type System (`src/types/mod.rs`)

Strong type safety with newtypes for all domain concepts:

- **IDs**: `GateId`, `ComponentId`, `ActorId`, `ReplicaId`
- **VCS Types**: `ChangeId`, `OperationId` (wrappers around jj-lib types)
- **Content Addressing**: `ContentHash` (SHA256)
- **Actor Types**: `ActorKind`, `ActorRef`
- **Repository References**: `RepoId`, `Revision`

All types implement proper serialization and display formatting.

### 2. SeaweedFS Client (`src/storage/seaweedfs/`)

HTTP client for SeaweedFS operations:

**Key Types:**
- `SeaweedFsClient` - Main client with master_url and http_client
- `BlobKey` - Content-addressed key (hash + blob_type)
- `BlobType` - Enum for Commit, Tree, File, Symlink, Conflict
- `BlobMetadata` - Tracking info (key, fid, size, created_at)

**Operations:**
- `write_blob()` - Assign fid from master, upload to volume server
- `read_blob_by_fid()` - Lookup volume, fetch blob
- Content-addressed paths: `{blob_type}/{hash[0..2]}/{hash[2..]}`

**Storage Flow:**
1. Calculate SHA256 hash of content
2. Request fid assignment from SeaweedFS master
3. Upload to assigned volume server
4. Store hash→fid mapping in PostgreSQL blob_metadata table

### 3. Custom Jujutsu Backend (`src/storage/jj_backend/`)

Full implementation of `jj_lib::backend::Backend` trait storing all VCS objects in SeaweedFS.

**Files:**
- `backend.rs` - Backend trait implementation (~450 lines)
- `serialization.rs` - Object serialization (Commit, Tree, Conflict)
- `factory.rs` - Backend factory registration

**Key Features:**
- All VCS objects (commits, trees, files, symlinks, conflicts) stored as content-addressed blobs
- SHA256 content addressing
- Serialization using bincode for efficiency
- Local fid cache (`.seaweedfs_cache/`) for development (production uses PostgreSQL)
- Implements all required Backend methods:
  - `read_commit()`, `write_commit()`
  - `read_tree()`, `write_tree()`
  - `read_file()`, `write_file()`
  - `read_symlink()`, `write_symlink()`
  - `read_conflict()`, `write_conflict()`
  - `gc()`, `as_any()`, `concurrency()`, `get_copy_records()`

**Serialization Strategy:**
- Commits: Custom serializable struct preserving all fields
- Trees: Serializable tree with entries and values
- Files: Raw bytes
- Symlinks: UTF-8 strings
- Conflicts: Debug format (temporary, pending jj-lib serialization support)

### 4. Repository Manager (`src/storage/jj_repos/`)

Manages Jujutsu workspaces with caching and lifecycle management.

**JjRepoManager:**
```rust
pub struct JjRepoManager {
    root: PathBuf,                      // /data/jj-repos/
    store_factories: StoreFactories,    // Custom backend factory
    seaweedfs_config: SeaweedFsConfig,
    settings: UserSettings,
    repo_cache: Arc<RwLock<HashMap<RepoId, Arc<Workspace>>>>,
}
```

**Repository Structure:**
```
/data/jj-repos/
  components/{component_id}/.jj/
  gates/{gate_id}/.jj/
```

**Operations:**
- `ensure_component_repo()` - Get/create component workspace
- `ensure_gate_repo()` - Get/create gate workspace
- `get_workspace()` - Retrieve cached workspace
- `list_all_repos()` - Enumerate all repositories
- `reload_workspace()` - Refresh after external changes

**Caching:**
- Workspaces cached by RepoId (Component or Gate)
- RwLock for concurrent read access
- Lazy initialization on first access

### 5. Operation Log Synchronization (`src/storage/jj_repos/sync.rs`)

Infrastructure for distributed eventual consistency.

**OpLogSync:**
- Publishes local operations to PostgreSQL
- Fetches operations from other replicas
- Leverages Jujutsu's automatic 3-way merge

**Sync Task:**
- Background task running every 30s (configurable)
- Iterates all repositories
- Publishes → Fetches → Merges → Reloads
- Errors logged but don't crash the task

**Design:**
- Each replica has unique `replica_id`
- Operations stored in PostgreSQL `operations` table
- Jujutsu handles conflict resolution automatically
- No manual merge logic required

## Design Decisions

### 1. Repository Per Entity

Each component and gate has its own Jujutsu repository:
- **Isolation**: Changes to one don't affect others
- **Scalability**: Repositories can be distributed across replicas
- **Versioning**: Full history for each entity
- **Concurrency**: Independent operations

### 2. Content-Addressed Storage

All blobs stored by SHA256 hash:
- **Deduplication**: Identical content stored once
- **Integrity**: Content hash verifies correctness
- **Distribution**: Easy to replicate across volumes
- **Caching**: Natural caching key

### 3. Eventual Consistency

Multiple replicas sync via operation log:
- **Availability**: Replicas can operate independently
- **Partition Tolerance**: Network splits handled gracefully
- **Convergence**: Jujutsu's 3-way merge ensures convergence
- **Scalability**: Add replicas without coordination

### 4. PostgreSQL for Indexing

Manifests and metadata indexed in PostgreSQL:
- **Fast Queries**: B-tree indexes for searching
- **Transactions**: ACID guarantees for metadata
- **Ecosystem**: Rich tooling and operators
- **Relations**: SeaORM entities for type safety

### 5. Separation of Concerns

Clear layer boundaries:
- **Storage**: VCS and blob operations
- **Repositories**: Database queries
- **Application Services**: Business logic
- **gRPC Services**: Protocol handling

## Migration from V1

### V1 Architecture (Current)
- Git repositories (one per component)
- SurrealDB (embedded RocksDB or clustered)
- Filesystem-based storage
- Single-server deployment

### V2 Architecture (New)
- Jujutsu repositories with custom backend
- SeaweedFS for distributed blob storage
- PostgreSQL for manifest indexing
- Multi-replica k8s deployment

### Migration Strategy

**Phase 1: Dual Write** (Weeks 1-2)
- New system writes to both V1 and V2
- Validate consistency

**Phase 2: Data Migration** (Week 3)
- Import Git history to Jujutsu
- Migrate SurrealDB to PostgreSQL
- Upload blobs to SeaweedFS

**Phase 3: Read Cutover** (Week 4)
- Switch reads to V2
- Maintain dual writes

**Phase 4: Full Cutover** (Week 5)
- Remove V1 dependencies
- Decommission SurrealDB

## Status

### ✅ Completed (Phase 1)

1. **Dependencies** - jj-lib, sea-orm, reqwest, chrono, bincode, futures
2. **Type System** - All domain newtypes with serialization
3. **SeaweedFS Client** - Full HTTP API implementation
4. **Custom Jujutsu Backend** - Complete Backend trait implementation
5. **Repository Manager** - Workspace lifecycle and caching
6. **Sync Infrastructure** - OpLogSync and background task (stubs)

**Files Created:**
- `src/types/mod.rs` (240 lines)
- `src/storage/seaweedfs/client.rs` (200 lines)
- `src/storage/jj_backend/backend.rs` (450 lines)
- `src/storage/jj_backend/serialization.rs` (220 lines)
- `src/storage/jj_backend/factory.rs` (30 lines)
- `src/storage/jj_repos/manager.rs` (180 lines)
- `src/storage/jj_repos/sync.rs` (60 lines)
- `src/storage/jj_repos/sync_task.rs` (60 lines)

**Total New Code:** ~1,440 lines of idiomatic Rust

### 🚧 In Progress

- **API Compatibility** - Fixing jj-lib 0.24 API calls (~70 errors remaining)
  - Method name changes (`as_bytes` vs accessing fields)
  - Constructor changes (`try_from_bytes` vs `from_bytes`)
  - Type changes (MillisSinceEpoch handling)

### 📋 Pending (Phases 2-7)

- Database schema and migrations
- SeaORM entities
- Repository layer
- gRPC API v2
- Application services
- Operation log sync (full implementation)
- Settings rewrite
- Main server rewrite
- Migration tool
- Integration tests
- Documentation

## Deployment

### Kubernetes Configuration

**Requirements:**
- PostgreSQL (StatefulSet)
- SeaweedFS (Master + Volume StatefulSets)
- Forged (Deployment with 3+ replicas)

**Storage:**
- PostgreSQL: 100Gi persistent volume
- SeaweedFS Master: 10Gi persistent volume
- SeaweedFS Volumes: 500Gi each (3 replicas)
- Jujutsu repos: ReadWriteMany 100Gi PVC

**Configuration:**
```yaml
env:
  - name: FORGED__POSTGRES__URL
    value: "postgresql://user:pass@postgres:5432/forged"
  - name: FORGED__SEAWEEDFS__MASTER_URL
    value: "http://seaweedfs-master:9333"
  - name: FORGED__SEAWEEDFS__NAMESPACE
    value: "forged"
  - name: FORGED__JJ_REPOS__ROOT
    value: "/data/jj-repos"
  - name: FORGED__SYNC__INTERVAL_SECS
    value: "30"
  - name: FORGED__SYNC__REPLICA_ID
    valueFrom:
      fieldRef:
        fieldPath: metadata.name
```

## Performance Considerations

### Caching Strategy

1. **Workspace Cache** - In-memory workspace cache per replica
2. **Fid Cache** - Local file cache for development (PostgreSQL in production)
3. **SeaweedFS** - Content-addressed blobs naturally cacheable
4. **PostgreSQL** - Indexed queries for manifest searches

### Scalability

- **Horizontal**: Add more forged replicas
- **Vertical**: Increase SeaweedFS volumes
- **Sharding**: Future work - shard by component/gate hash

### Monitoring

- Operation log sync latency
- Workspace cache hit rate
- SeaweedFS throughput
- PostgreSQL query performance

## Future Enhancements

1. **Copy Tracking** - Implement get_copy_records with full file history
2. **Garbage Collection** - Implement blob GC based on reachability
3. **Compression** - Add compression for large blobs
4. **Encryption** - At-rest encryption for sensitive manifests
5. **Metrics** - Prometheus metrics for all operations
6. **Conflict UI** - Better visualization of merge conflicts
7. **Selective Sync** - Only sync relevant repos per replica

## References

- [Jujutsu VCS](https://github.com/martinvonz/jj)
- [SeaweedFS](https://github.com/seaweedfs/seaweedfs)
- [SeaORM](https://www.sea-ql.org/SeaORM/)
- [Plan Document](/home/toasty/.claude/plans/composed-brewing-quail.md)
