# Forge

Forge is an experimental, service-oriented code forge and packaging platform. It explores modern transports for Git (gRPC-like over QUIC), structured storage for metadata (SurrealDB), and a simple server binary called `forged`. This repository is a Rust workspace that contains the server, supporting crates, a web UI scaffold, and deployment artifacts (Dockerfile and a Helm chart).

Status: early, pre-alpha. Expect rough edges.

- Server crate: `crates/forged` (binary: `forged`)
- Web UI (SvelteKit scaffold): `web/`
- Helm chart: `charts/forged/`
- Sample data for tests: `sample_data/`

## Quick start

Choose one of the options below.

### 1) Download a release (recommended)

The easiest way to try Forge is to download the `forged` binary from this repository’s Releases page.

1. Go to the Releases page of this repo and pick the latest release.
   - In GitHub UI: open this repository and click “Releases” in the right sidebar.
   - Or open the Releases page for this repository’s URL.
2. Download the asset matching your OS/architecture.
3. Unpack it and run:

```
./forged
```

By default, the server listens on 127.0.0.1:50051. See Configuration below for changing ports, storage, and more.

Tip: With GitHub CLI you can fetch artifacts programmatically (adjust pattern to match assets on the Release):

```
# example: fetch latest linux x86_64 tarball
# gh release download -R toasterson/forge -p "forged-*-x86_64-unknown-linux-gnu.tar.gz"
```

### 2) Run with Docker

If you prefer containers:

```
docker run --rm -it \
  -p 50051:50051 \
  -e FORGED__SERVER__LISTEN_ADDR=0.0.0.0:50051 \
  -e FORGED__SURREAL__MODE=embedded \
  -e FORGED__SURREAL__PATH=/data/surreal \
  -e FORGED__REPOS__MODE=fs \
  -e FORGED__REPOS__ROOT=/data/repos \
  -v $(pwd)/data:/data \
  ghcr.io/toasterson/forge:latest
```

See docs/DEPLOYMENT.md for more options including Kubernetes (Helm) and systemd.

### 3) Build from source

Prerequisites:
- Rust toolchain (stable), Cargo
- System libraries: libarchive-dev (for pkgdev via compress-tools), pkg-config, OpenSSL headers (libssl-dev)
- Optional: Node.js for the web UI dev workflow

Build everything:

```
cargo build
```

Run the server:

```
cargo run -p forged
```

Enable the optional QUIC endpoint stub:

```
cargo run -p forged --features quic
# then set at runtime to actually start it:
# FORGED_QUIC_ADDR=127.0.0.1:50052 ./target/debug/forged
```

Export telemetry to an OTLP collector (feature-gated):

```
cargo run -p forged --features otel
# at runtime set: OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
```

## Configuration

Configuration is loaded in this precedence order (see crates/forged/src/settings.rs):
1. If `FORGED_CONFIG` is set, load that file (error if it does not exist).
2. Else, if a `./forged.toml` exists in the current working directory, load it.
3. Finally, apply environment overrides (always allowed) and defaults.

Additional address fallback: the binary also accepts `FORGED_ADDR` as a late override for `server.listen_addr`.

Useful env vars (examples):

```
# Network
FORGED__SERVER__LISTEN_ADDR=127.0.0.1:50051
# Storage: SurrealDB
FORGED__SURREAL__MODE=embedded            # or "clustered"
FORGED__SURREAL__PATH=./data/surreal      # for embedded
FORGED__SURREAL__ENDPOINT=ws://127.0.0.1:8000  # for clustered
FORGED__SURREAL__USERNAME=root
FORGED__SURREAL__PASSWORD=secret
FORGED__SURREAL__NAMESPACE=forged
FORGED__SURREAL__DATABASE=default
# Git repository storage
FORGED__REPOS__MODE=fs                    # or "s3"
FORGED__REPOS__ROOT=./data/repos          # when mode = fs
# SMTP (optional)
FORGED__SMTP__HOST=smtp.example.com
FORGED__SMTP__PORT=587
FORGED__SMTP__USERNAME=forge@example.com
FORGED__SMTP__PASSWORD=... # set via env
FORGED__SMTP__FROM=forge@example.com
FORGED__SMTP__STARTTLS=true
```

Minimal forged.toml example you can place next to the binary:

```toml
[server]
listen_addr = "127.0.0.1:50051"

[surreal]
# One of: "embedded" (rocksdb) or "clustered"
mode = "embedded"
path = "./data/surreal"

[repos]
# One of: "fs" or "s3"
mode = "fs"
root = "./data/repos"

# [smtp]
# host = "smtp.example.com"
# port = 587
# username = "forge@example.com"
# password = "<set via env>"
# from = "forge@example.com"
# starttls = true
```

## Deployment

See the full deployment guide at docs/DEPLOYMENT.md.
It covers:
- Binary deployments (Linux/macOS) with systemd example
- Docker and Docker Compose
- Kubernetes via `charts/forged` (Helm)
- Feature flags (QUIC, OTEL), logging, and persistence

## Testing and development

- Run all tests: `cargo test`
- Run tests for a specific crate: `cargo test -p forged` or `cargo test -p pkgdev`
- The server’s logging uses `tracing`. Control verbosity with `RUST_LOG`, e.g.:
  `RUST_LOG=forged=debug,forged::services=trace`

### Cross-compiling for illumos (x86_64)

This repository includes a Cross.toml to build for illumos using the cross tool.

Steps:
- Install cross: `cargo install cross` (or use the `taiki-e/install-action` in CI)
- Build forged and pkgdev for illumos:
  - Debug: `cross build --target x86_64-unknown-illumos -p forged -p pkgdev`
  - Release: `cross build --target x86_64-unknown-illumos -p forged -p pkgdev --release`

The configuration uses the cross-rs Docker image for illumos and requires Docker on the host.

## License

MPL-2.0
