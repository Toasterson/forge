# Phase 2: Harden for Production

**Status**: Complete
**Started**: 2026-03-22
**Completed**: 2026-03-22

## Items

| # | Gap | Status | Files Modified |
|---|-----|--------|----------------|
| 2.1 | Circuit breaker + graceful degradation | done | `circuit_breaker.rs` (new), `build_report_consumer.rs` (exponential backoff) |
| 2.2 | Audit logging | done | `migration/m20260322_000001_create_audit_log.rs`, `entities/audit_log.rs`, `services/audit_service.rs`, `app_state.rs` |
| 2.3 | SeaweedFS client hardening | done | `storage/seaweedfs/client.rs` (retries, timeouts, health_check), `settings.rs` |
| 2.4 | Concurrency limiting | done | `transport/grpc/mod.rs` (ConcurrencyLimitLayer + per-connection limit) |
| 2.5 | Reliable AMQP publish | done | `services/build_dispatch.rs` (3x retry, marks job failed on exhaustion) |
| 2.6 | Blob download ACL | done | `build_service.rs`, `source_archive_repository.rs` |
| 2.7 | Admin CLI | done | `admin.rs` (new) -- MigrateStatus, BuildCleanup commands |
| 2.8 | Backup/restore tooling | done | `scripts/backup.sh`, `scripts/restore.sh` |
