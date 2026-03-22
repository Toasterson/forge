# Phase 3: Operational Maturity

**Status**: Complete
**Started**: 2026-03-22
**Completed**: 2026-03-22

## Items

| # | Gap | Status | Files Modified |
|---|-----|--------|----------------|
| 3.1 | Metrics export (Prometheus) | deferred | Need separate HTTP server for /metrics; recommend post-MVP |
| 3.2 | Request ID propagation | done | `transport/grpc/middleware.rs` (UUID request ID in tracing span + response headers) |
| 3.3 | Input validation tightening | done | `component_service.rs`, `gate_service.rs` (name 256 char, KDL 1MB limits) |
| 3.4 | DB connection pool tuning | done | `settings.rs` (connect_timeout_secs, idle_timeout_secs, min_connections) |
| 3.5 | xtask update (remove SurrealDB refs) | done | `crates/xtask/src/` updated |
| 3.6 | Helm chart | done | `charts/forged/` (Chart.yaml, values.yaml, deployment.yaml, service.yaml) |
| 3.7 | Dockerfile HEALTHCHECK | done | `Dockerfile` (TCP check on 50051) |
| 3.8 | JWKS cache forced refresh | done | `services/oidc_service.rs` (refresh on unknown kid before failing) |
| 3.9 | List operation access filtering | deferred | Needs RBAC-aware list queries; recommend post-MVP |

## Notes

- Prometheus metrics (#3.1) deferred: requires a separate HTTP server alongside gRPC, which is architectural work beyond the current scope
- List filtering (#3.9) deferred: would require changing all list queries to join with gate_member table for access checks, significant query complexity
- JWKS refresh now does a forced re-fetch when a token's kid is not found in cache, closing the 1-hour window for key rotation
