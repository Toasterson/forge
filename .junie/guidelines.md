# Project Guidelines for Junie

This document provides guidelines for Junie (JetBrains AI Coding Agent) when working on the forge Rust codebase.

### Dependencies
The project uses several key dependencies:
- **Error handling**: thiserror and miette. Use WrapErr to enhance error handling.
- **Serialization**: serde and serde_json
- **Logging**: tracing and tracing-subscriber
- **Configuration**: config
- **DSL for Builds** kdl
- **Compression**: flate2 and lz4
- **Versioning**: semver
- **CLI**: clap

### Dependencies Setup

**For Application Crates:**
```toml
[dependencies]
miette = { version = "7.6.0", features = ["fancy"] }
thiserror = "1.0.50"
tracing = "0.1.37"
tracing-subscriber = "0.3.17"
```

**For Library Crates:**
```toml
[dependencies]
miette = "7.6.0"
thiserror = "1.0.50"
tracing = "0.1.37"
```

**Rule:** Only enable the "fancy" feature in top-level application crates, not in library crates.

### Error Type Definition

Define error types as enums using thiserror and miette's Diagnostic derive macro:

```rust
use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
#[error("A validation error occurred")]
#[diagnostic(
    code(ips::validation_error),
    help("Please check the input data and try again.")
)]
pub enum ValidationError {
    // Error variants go here
}
```

### Error Code Naming Convention

Use a consistent naming scheme for error codes:
- Top-level errors: `ips::category_error` (e.g., `ips::validation_error`)
- Specific errors: `ips::category_error::specific_error` (e.g., `ips::validation_error::invalid_name`)

Examples:
- `ips::validation_error`
- `ips::validation_error::invalid_name`

### Error Handling in Library vs. Application Code

- In library code (like `libips`), always return specific error types
- In application code, you can use `miette::Result` for convenience
- In application code, you can enable the "fancy" feature in miette to get more information about the error
- In application code, you can wrap std::io::Error with transparent error
- In application code, you can wrap external library errors with transparent error or convert with From
- Use WrapErr to enhance error information with contextual information

### Decision Tree for Error Handling

1. **Is this a library or application crate?**
   - **Library**: Use specific error types, don't use miette's "fancy" feature
   - **Application**: Can use miette::Result for convenience, enable "fancy" feature

2. **What type of error is being handled?**
   - **Input validation**: Create specific error variants with helpful messages
   - **I/O operations**: Wrap std::io::Error with transparent error
   - **Parsing**: Include source highlighting with NamedSource and SourceSpan
   - **External library errors**: Wrap with transparent error or convert with From

3. **How should the error be propagated?**
   - **Within same error type**: Use ? operator
   - **Between different error types**: Use map_err or implement From trait

4. **What level of diagnostic information is needed?**
   - **Basic**: Just use #[error] attribute
   - **Medium**: Add #[diagnostic] with code and help
   - **Detailed**: Include source_code, label, and related information

For more detailed guidelines on error handling, refer to:
- `./doc/rust_docs/error_handling.md`

## Testing Guidelines

### Running Tests

To run tests for the entire project:
```bash
cargo test
```

To run tests for a specific crate:
```bash
cargo test -p <crate_name>
```

### Writing Tests

- Unit tests should be placed in the same file as the code they're testing, in a `mod tests` block
- Integration tests should be placed in the `tests` directory of each crate
- End-to-end tests should use the test environment set up by `cargo xtask setup-test-env`

## Build Guidelines

### Building the Project

Using cargo directly:
```bash
cargo build                    # Build the entire project
cargo build -p <crate_name>    # Build a specific crate
cargo build --release          # Build with optimizations for release
```

Using cargo-xtask:
```bash
cargo xtask build              # Build the entire project
cargo xtask build -p <crate_name>  # Build a specific crate
cargo xtask build -r           # Build with optimizations for release
```

This order is important as it reflects the dependency hierarchy, with `libips` being the foundation that other crates build upon.

### Adding New Commands

To add a new command to cargo-xtask:

1. Edit the `xtask/src/main.rs` file
2. Add a new variant to the `Commands` enum
3. Implement a function for the new command
4. Add a match arm in the `main` function to call your new function

## Code Style Guidelines

### General Guidelines

- Follow the Rust standard style guide
- Use `cargo fmt` to format code
- Use `cargo clippy` to check for common mistakes and improve code quality

### Naming Conventions

- Use snake_case for variables, functions, and modules
- Use CamelCase for types, traits, and enums
- Use SCREAMING_SNAKE_CASE for constants
- Prefix unsafe functions with `unsafe_`

### Documentation

- Document all public items with doc comments
- Include examples in doc comments where appropriate
- Document error conditions and return values

### Error Handling

- Follow the error handling guidelines above
- Use the ? operator for error propagation where appropriate
- Avoid using `unwrap()` or `expect()` in production code

### Logging

- Use the tracing crate for logging
- Use appropriate log levels:
  - `trace`: Very detailed information
  - `debug`: Useful information for debugging
  - `info`: General information about the application's operation
  - `warn`: Potentially problematic situations
  - `error`: Error conditions

## Workflow for Junie

When working on the IPS project, Junie should follow this workflow:

1. **Understand the Issue**: Thoroughly review the issue description and related code
2. **Plan the Changes**: Create a plan for implementing the changes
3. **Implement the Changes**: Make the necessary changes to the code
4. **Test the Changes**: Run tests to ensure the changes work as expected
5. **Document the Changes**: Update documentation as needed
6. **Submit the Changes**: Submit the changes for review

When implementing error handling, Junie should follow the error handling guidelines above and use the decision tree to determine the appropriate approach.



## Forge project specifics (build, config, testing)

This section documents repository-specific know-how for the forge workspace.

### Build and run
- Build entire workspace: `cargo build`.
- Build a specific crate: `cargo build -p <crate>`.
- Run the forged server locally: `cargo run -p forged`.
- Optional features for `forged`:
  - `quic`: enables a QUIC endpoint stub. Run with: `cargo run -p forged --features quic`. At runtime set `FORGED_QUIC_ADDR="127.0.0.1:50052"` to start it; if unset, QUIC stays disabled.
  - `otel`: enables OpenTelemetry export. Set `OTEL_EXPORTER_OTLP_ENDPOINT` to your collector (e.g., `http://localhost:4317`).

### Configuration model (crates/forged)
Settings loading (see `crates/forged/src/settings.rs::Settings::load`) resolves configuration in this precedence order:
1. If `FORGED_CONFIG` is set, load that file (error if it does not exist).
2. Else, if a `./forged.toml` exists in the current working directory, load it.
3. Finally, apply environment overrides (always allowed) and defaults.

Environment overrides use the `FORGED__` prefix with `__` as separator between nested fields. Useful keys:
- `FORGED__SERVER__LISTEN_ADDR` — TCP gRPC bind address, e.g., `127.0.0.1:50051`.
- SurrealDB (storage):
  - `FORGED__SURREAL__MODE` — `embedded` (rocksdb) or `clustered` (remote), default: `embedded`.
  - `FORGED__SURREAL__ENDPOINT` — remote endpoint for clustered mode, default: `ws://127.0.0.1:8000`.
  - `FORGED__SURREAL__USERNAME`, `FORGED__SURREAL__PASSWORD` — credentials for clustered mode.
  - `FORGED__SURREAL__NAMESPACE`, `FORGED__SURREAL__DATABASE` — logical db selection, defaults: `forged` / `default`.
  - `FORGED__SURREAL__PATH` — embedded rocksdb dir, default: `./data/surreal`.
- SMTP (optional notification email):
  - `FORGED__SMTP__HOST`, `FORGED__SMTP__PORT`, `FORGED__SMTP__USERNAME`, `FORGED__SMTP__PASSWORD`, `FORGED__SMTP__FROM`, `FORGED__SMTP__STARTTLS`.

Additional address fallback: `FORGED_ADDR` may be used by the binary to override `server.listen_addr` just before parsing; if neither is set, the default is `127.0.0.1:50051`.

Example minimal `forged.toml` (use via `FORGED_CONFIG=/abs/path/forged.toml`):
```toml
[server]
listen_addr = "127.0.0.1:50051"

[surreal]
# One of: "embedded" (rocksdb) or "clustered"
mode = "embedded"
path = "./data/surreal"

# For clustered mode:
# endpoint = "ws://127.0.0.1:8000"
# username = "root"
# password = "secret"
# namespace = "forged"
# database = "default"

# [smtp]
# host = "smtp.example.com"
# port = 587
# username = "forge@example.com"
# password = "<set via env>"
# from = "forge@example.com"
# starttls = true
```

### Logging / telemetry
- Logging is provided by `tracing` with `tracing-subscriber::EnvFilter`.
  - Set `RUST_LOG` to control verbosity, e.g.: `RUST_LOG=forged=debug,forged::services=trace`.
  - Default level is `info` if `RUST_LOG` is not set.
- With `--features otel` and `OTEL_EXPORTER_OTLP_ENDPOINT` set, spans are exported over OTLP in addition to stdout formatting.

### Storage: SurrealDB connection
Implementation is in `crates/forged/src/storage/surreal.rs`:
- Embedded mode uses RocksDB via URI `rocksdb:<path>` with default `./data/surreal`.
- Clustered mode uses the `ws://` protocol; if `username` and `password` are set, a `Root` signin is performed.
- After connect, the code selects namespace and database specified in settings (defaults: `forged` / `default`).

### Testing in this workspace
- Run all tests: `cargo test`.
- Run tests for one crate: `cargo test -p forged` or `cargo test -p pkgdev`.
- Existing integration tests under `crates/pkgdev/tests` use data in `sample_data/` (no external services required). The full workspace test run passes in a clean checkout.

Caveats for config in tests:
- Tests execute with the crate directory as CWD. The settings loader looks for `./forged.toml` relative to the CWD; the workspace’s top-level `forged.toml` is not seen by crate tests unless `FORGED_CONFIG` points to it. Environment overrides (`FORGED__...`) are the recommended way inside tests.

#### Example: temporary unit test we validated
We validated the following pattern locally (add to any file in `crates/forged/src/` within a `#[cfg(test)] mod tests` block):
```rust
#[test]
fn env_overrides_listen_addr() {
    use std::env;
    let key = "FORGED__SERVER__LISTEN_ADDR";
    let val = "127.0.0.1:12345";
    let prev = env::var(key).ok();
    env::set_var(key, val);
    let settings = crate::settings::Settings::load().expect("load settings");
    assert_eq!(settings.server.listen_addr.as_deref(), Some(val));
    if let Some(p) = prev { env::set_var(key, p); } else { env::remove_var(key); }
}
```
Run it with:
```bash
cargo test -p forged -- env_overrides_listen_addr
```
We added, ran, and then removed this test during verification to keep the repo clean.

### Additional development notes
- Do not commit real secrets. The repository’s top-level `forged.toml` is for local development; prefer keeping a private config outside VCS and point at it with `FORGED_CONFIG` or inject secrets via env variables.
- When adding new CLI flags or configuration fields, update `Settings` in `crates/forged/src/settings.rs` and mirror them in the example `forged.toml` here.
- If enabling QUIC or OTEL features in CI, ensure the feature flags are wired in the corresponding cargo invocations.
- For noisy modules, prefer per-module `RUST_LOG` selectors to keep logs actionable during tests.
