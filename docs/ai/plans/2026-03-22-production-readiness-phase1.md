# Phase 1: Unblock Production

**Status**: Complete
**Started**: 2026-03-22
**Completed**: 2026-03-22

## Items

| # | Gap | Status | Files Modified |
|---|-----|--------|----------------|
| 1.1 | TLS + ACME for gRPC | done | `Cargo.toml`, `settings.rs`, `acme.rs` (new), `transport/grpc/mod.rs`, `main.rs`, `lib.rs` |
| 1.2 | Fix build dispatch metadata | done | `services/build_dispatch.rs`, `app_state.rs` |
| 1.3 | Upload size limits | done | `settings.rs`, `transport/grpc/component_service.rs`, `main.rs` |
| 1.4 | Pagination on all list RPCs | done | `proto/api_v2.proto`, `pagination.rs` (new), all 6 repos, `gate_service.rs`, `component_service.rs`, `build_service.rs` |
| 1.5 | Health check endpoint | done | `transport/grpc/mod.rs` (HealthCheckDeps, tonic-health service + background checker) |
| 1.6 | SMF service manifest | done | `smf/forged.xml` (new), `smf/forged-method` (new) |

## Notes

- TLS supports three modes: `none` (dev), `manual` (PEM files), `acme` (Let's Encrypt HTTP-01)
- ACME uses `instant-acme` 0.8 with `rcgen` for CSR generation; certs cached on disk
- TLS-ALPN-01 deferred (requires tonic upgrade for custom rustls ServerConfig access)
- DNS-01 via Gandi.net API deferred to separate follow-up
- Health check probes PostgreSQL (`ping`) and SeaweedFS (`/cluster/status`) every 10s
- Pagination uses cursor-based approach (base64-encoded `created_at` timestamp)
- Upload size limit defaults to 2 GiB, configurable via `server.max_upload_size`
