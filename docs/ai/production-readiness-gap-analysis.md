# Production Readiness on illumos

This chapter documents the gap analysis for deploying Forge in production on illumos. It identifies what exists, what's missing, and what must be addressed before production use.

## Executive Summary

Forge's core functionality (gRPC API, RBAC, component/gate management, build dispatch) is implemented. However, several areas critical for production are missing or incomplete. The gaps fall into three tiers:

- **Blocking** -- Must fix before any production deployment
- **Major** -- Should fix before production; creates operational risk if deferred
- **Minor** -- Improvements that reduce operational burden

## Blocking Issues

### 1. No TLS for gRPC Transport

The gRPC server binds plaintext HTTP/2 on port 50051. All traffic -- including authentication tokens, SSH keys, and source archives -- travels unencrypted.

**Current state:** `tonic::transport::Server::builder()` without `.tls_config()`

**Required:**
- Add `rustls` or `openssl` TLS configuration to the tonic server
- Support certificate and key file paths in `forged.toml`
- Consider mutual TLS (mTLS) for service-to-service communication
- Add TLS for PostgreSQL and SeaweedFS connections

**Configuration needed in `settings.rs`:**
```toml
[server.tls]
cert_file = "/etc/forged/tls/server.crt"
key_file = "/etc/forged/tls/server.key"
ca_file = "/etc/forged/tls/ca.crt"      # for mTLS
require_client_cert = false
```

### 2. Build Dispatch Metadata Empty

`BuildDispatchService` constructs `JobRequest` with empty `repo_url`, `repo_owner`, `repo_name`, and `commit_sha` fields. Solstice CI workers receive no context about what to build.

**Impact:** The entire build pipeline is non-functional.

**Fix:** Populate job request metadata from the component and gate records before AMQP publish.

### 3. No Upload Size Limits

`UploadSourceArchive` and `UploadComponentFile` accept arbitrary `total_size` values with no upper bound. A malicious client can trigger OOM by declaring a multi-petabyte upload.

**Required:**
- Add `max_upload_size` to server configuration (e.g., 2 GiB default)
- Reject uploads exceeding the limit before buffering
- Stream uploads to disk or SeaweedFS instead of accumulating in memory

### 4. No Pagination on List Operations

All repository `list_*` methods return unbounded result sets. With thousands of components, a single `ListComponents` RPC can exhaust server memory.

**Affected operations:** `list_all()` (gates), `list_by_owner()`, `list_members()`, `list_by_gate()` (components), `list_builds()`

**Required:**
- Add `limit` and `offset` (or cursor-based) pagination to all list RPCs
- Default page size (e.g., 50) with configurable maximum (e.g., 1000)
- Update protobuf messages with pagination fields

### 5. No Health Check Endpoint

No gRPC health service or HTTP health endpoint exists. Orchestrators (SMF, Kubernetes) cannot determine if the server is ready to accept traffic.

**Required:**
- Implement the [gRPC Health Checking Protocol](https://github.com/grpc/grpc/blob/master/doc/health-checking.md) via `tonic-health`
- Probe PostgreSQL and SeaweedFS connectivity in the readiness check
- Add an HTTP `/healthz` endpoint for simpler monitoring

### 6. No SMF Service Manifest

illumos uses the Service Management Framework (SMF) for service lifecycle. Without a manifest, `forged` cannot be managed as a proper system service.

**Required files:**

- `smf/forged.xml` -- SMF service manifest
- `smf/forged-method` -- Start/stop method script

**Manifest should declare:**
- Dependencies on `network:default` and `postgresql:default`
- Start method pointing to the `forged` binary
- Environment variables for config path and log level
- Authorization for `solaris.smf.manage.forge`
- Log file paths under `/var/log/forged/`

## Major Issues

### 7. No Graceful Degradation

When any backend (PostgreSQL, SeaweedFS, RabbitMQ, OIDC provider) is unavailable, all operations fail immediately. There is no circuit breaking, fallback, or read-only mode.

**Required:**
- Circuit breaker for SeaweedFS and AMQP operations (e.g., `tower` middleware or manual state machine)
- Cached JWKS fallback when OIDC provider is temporarily unreachable
- Read-only mode when write backends are down
- Exponential backoff for `BuildReportConsumer` reconnection (currently fixed 5-second interval)

### 8. No Audit Logging

User actions (create, update, delete, permission changes) are not logged in an audit trail. Only standard `tracing` logs exist.

**Required:**
- `audit_log` database table (actor_id, action, resource_type, resource_id, timestamp, result, details)
- Log all mutations in the service layer
- Log all permission check failures
- Separate audit log configuration from operational logging

### 9. SeaweedFS Client Not Production-Ready

The SeaweedFS HTTP client has no retries, no configurable timeouts, no circuit breaker, and no health checks. A transient network error causes immediate request failure.

**Required:**
- Configurable HTTP timeouts (connect, read, write)
- Retry with exponential backoff for transient failures (5xx, timeout, connection reset)
- Health check on startup (verify master is reachable)
- Blob deletion support (currently blobs accumulate forever)

### 10. No Rate Limiting

No protection against rapid successive requests. A single client can monopolize server resources.

**Required:**
- Tower-based rate limiting middleware (e.g., `tower-governor`)
- Per-actor and global rate limits
- Configurable via `forged.toml`

### 11. AMQP Publish Not Reliable

Build job records are created in PostgreSQL before AMQP publish. If publish fails, orphaned job records remain in "pending" state forever.

**Required:**
- Transactional outbox pattern: write job + outbox record in one DB transaction, publish asynchronously
- Or: retry AMQP publish with idempotency key
- Dead-letter handling for failed builds

### 12. Blob Download Permission Model

Any authenticated user can download any blob if they know the SHA-256 hash. There is no per-blob ownership or access control.

**Required:**
- Verify the requesting actor has read access to a component that references the blob
- Or: generate time-limited, signed download URLs

### 13. No Admin CLI

No commands exist for operational maintenance: user suspension, token revocation, build job cleanup, database integrity checks.

**Required:** Either extend `forged` with admin subcommands or create a separate `forged-admin` binary:

```
forged-admin actor list
forged-admin actor suspend <ID>
forged-admin actor delete <ID>
forged-admin build cleanup --older-than 30d
forged-admin db check-integrity
forged-admin db migrate-status
```

### 14. No Backup/Restore Tooling

No scripts or documentation for backing up PostgreSQL, SeaweedFS, or Jujutsu repository data.

**Required:**
- Backup script coordinating `pg_dump`, SeaweedFS volume export, and JJ repo archival
- Restore procedure documentation
- Backup verification (test restores)

## Minor Issues

### 15. No Metrics Export

OpenTelemetry tracing is available via the `otel` feature flag, but no Prometheus metrics endpoint exists. No request latency, error rate, or pool utilization metrics.

**Required:**
- Prometheus metrics endpoint (e.g., `/metrics` via `metrics-exporter-prometheus`)
- Key metrics: request latency (p50/p95/p99), error rate, active connections, pool utilization, blob storage usage

### 16. No Request ID Propagation

Requests cannot be traced across service boundaries. No correlation ID in logs.

**Required:**
- Generate a unique request ID in the gRPC middleware
- Propagate via gRPC metadata
- Include in all log lines

### 17. Input Validation Gaps

- No string length limits on `display_name`, `email`, `gate_kdl`, `recipe_kdl`
- No UUID format validation on ID fields (accepted as opaque strings)
- KDL content (`gate_kdl`, `recipe_kdl`) accepted without parse validation at the transport layer

### 18. Database Connection Pool Tuning

The pool has a hardcoded 20-connection default with no idle timeout, no health check queries, and no pool exhaustion handling.

**Required:**
- Configurable `min_connections`, `max_connections`, `connect_timeout`, `idle_timeout`
- Connection health check query (`SELECT 1`)

### 19. xtask Uses Stale Architecture

The `xtask` crate still references SurrealDB (v1 architecture). It should be updated to match the current PostgreSQL-based setup.

### 20. Helm Chart Missing

Documentation references `charts/forged/` but the directory does not exist. Either create the chart or remove the documentation reference.

### 21. Dockerfile Missing HEALTHCHECK

The Dockerfile has no `HEALTHCHECK` instruction, so Docker cannot determine container health.

### 22. JWKS Cache Invalidation

The OIDC JWKS cache has a 1-hour TTL with no forced refresh. If the identity provider rotates keys, clients may be denied for up to an hour.

### 23. List Operations Leak Data

`ListGates` returns all gates regardless of the requesting actor's access. `ListMembers` exposes full permission details to anyone with gate read access.

## Gap Summary

| # | Gap | Severity | Effort |
|---|-----|----------|--------|
| 1 | No TLS for gRPC | **Blocking** | Medium |
| 2 | Build dispatch metadata empty | **Blocking** | Small |
| 3 | No upload size limits | **Blocking** | Small |
| 4 | No pagination | **Blocking** | Medium |
| 5 | No health check endpoint | **Blocking** | Small |
| 6 | No SMF manifest | **Blocking** | Small |
| 7 | No graceful degradation | Major | Large |
| 8 | No audit logging | Major | Medium |
| 9 | SeaweedFS client fragile | Major | Medium |
| 10 | No rate limiting | Major | Medium |
| 11 | AMQP publish unreliable | Major | Medium |
| 12 | Blob download unscoped | Major | Medium |
| 13 | No admin CLI | Major | Large |
| 14 | No backup/restore | Major | Medium |
| 15 | No metrics export | Minor | Medium |
| 16 | No request ID propagation | Minor | Small |
| 17 | Input validation gaps | Minor | Small |
| 18 | DB pool tuning | Minor | Small |
| 19 | xtask stale | Minor | Small |
| 20 | Helm chart missing | Minor | Medium |
| 21 | Dockerfile no HEALTHCHECK | Minor | Small |
| 22 | JWKS cache invalidation | Minor | Small |
| 23 | List operations leak data | Minor | Small |

## Recommended Sequence

**Phase 1: Unblock production** (items 1-6)
1. Add TLS configuration to gRPC server
2. Fix build dispatch metadata
3. Add upload size limits
4. Implement pagination on all list RPCs
5. Add gRPC health check service
6. Create SMF manifest and method script

**Phase 2: Harden for production** (items 7-14)
7. Add circuit breakers and graceful degradation
8. Implement audit logging
9. Harden SeaweedFS client (retries, timeouts)
10. Add rate limiting middleware
11. Make AMQP publish reliable (outbox pattern)
12. Scope blob downloads to component access
13. Build admin CLI
14. Create backup/restore tooling and procedures

**Phase 3: Operational maturity** (items 15-23)
15. Add Prometheus metrics
16. Implement request ID propagation
17. Tighten input validation
18. Tune database connection pool
19. Update xtask to current architecture
20. Create or remove Helm chart
21. Add Dockerfile HEALTHCHECK
22. Improve JWKS cache refresh
23. Filter list operations by actor access
