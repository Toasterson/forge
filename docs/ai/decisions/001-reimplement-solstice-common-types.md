# ADR 001: Re-implement Solstice CI Common Types Instead of Git Dependency

**Date**: 2026-02-16
**Status**: Accepted

## Context

Phase 5 requires forged to dispatch build jobs to Solstice CI via RabbitMQ. The solstice-ci project provides a `common` crate with shared message types (`JobRequest`, `JobResult`, `MqConfig`) and AMQP helper functions.

However, the solstice-ci repository at `codeberg.org/Toasterson/solstice-ci` uses SHA256 git object format. Cargo cannot fetch from SHA256 git repos due to [rust-lang/cargo#14942](https://github.com/rust-lang/cargo/issues/14942) (blocked on libgit2 and gitoxide support).

## Decision

Re-implement wire-compatible message types (`JobRequest`, `JobResult`) and AMQP configuration directly in `services/build_dispatch.rs` rather than depending on the solstice-ci common crate.

## Alternatives Considered

1. **Git dependency**: Blocked by cargo#14942 (SHA256 object format).
2. **Vendor the crate**: Requires cloning locally with a SHA256-capable git client, fragile maintenance burden.
3. **Git submodule**: Same SHA256 issue for some git clients; adds submodule management complexity.
4. **Publish to crates.io**: Requires upstream action; not in our control.
5. **Mirror to SHA1 repo**: Fragile, not well-tested tooling.

## Consequences

- Message types serialize to the same JSON schema as solstice-ci's types, maintaining wire compatibility.
- AMQP patterns (deadpool-lapin, direct exchanges, durable queues) match existing forge codebase conventions (worker, ghwhrecv).
- When cargo gains SHA256 support, we can optionally switch to the git dependency and remove the local types.
- Risk: schema drift if solstice-ci changes its message format. Mitigation: the `schema_version` field in messages enables version detection.
