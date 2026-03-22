# Installation

## Pre-built Binaries

Download the latest release for your platform from the [releases page](https://github.com/OpenFlowLabs/forge/releases).

The release includes two binaries:

- `forged` -- the server
- `pkgdev` -- the CLI tool for package developers

## Docker

Run the server using the official Docker image:

```bash
docker run -p 50051:50051 ghcr.io/openflowlabs/forged:latest
```

## Building from Source

### Prerequisites

- Rust stable toolchain (1.83+)
- `protobuf-compiler`
- `clang`
- `pkg-config`
- `libssl-dev`
- `libarchive-dev`

On Debian/Ubuntu:

```bash
sudo apt install protobuf-compiler clang pkg-config libssl-dev libarchive-dev
```

### Build

```bash
git clone https://github.com/OpenFlowLabs/forge.git
cd forge
cargo build --workspace --release
```

The binaries are placed in `target/release/forged` and `target/release/pkgdev`.

### Cross-compiling for illumos

Use the `cross` tool with the illumos sysroot:

```bash
cargo install cross
cargo run -p xtask -- sysroot  # generate illumos sysroot
cross build --target x86_64-unknown-illumos -p forged -p pkgdev --release
```

### Optional Features

The `forged` crate supports these feature flags:

- **`otel`** -- Enable OpenTelemetry trace and metric export
- **`quic`** -- Enable a QUIC transport endpoint alongside gRPC

```bash
cargo build -p forged --release --features otel,quic
```
