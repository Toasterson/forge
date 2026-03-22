# Building from Source

## Prerequisites

Install the required system dependencies:

**Debian/Ubuntu:**

```bash
sudo apt install protobuf-compiler clang pkg-config libssl-dev libarchive-dev
```

**illumos:**

```bash
pkg install developer/build/gnu-make developer/gcc-13 library/security/openssl-31
```

Install the Rust toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Building

```bash
# Build the entire workspace
cargo build --workspace

# Build in release mode
cargo build --workspace --release

# Build only the server
cargo build -p forged

# Build only the CLI
cargo build -p pkgdev
```

## Running

```bash
# Run the server (development mode)
cargo run -p forged

# Run with specific log level
RUST_LOG=forged=debug cargo run -p forged
```

## Linting

```bash
# Check formatting
cargo fmt --all -- --check

# Run clippy
cargo clippy --workspace -- -D warnings
```

## Cross-compilation for illumos

Generate the illumos sysroot and cross-compile:

```bash
cargo install cross
cargo run -p xtask -- sysroot
cross build --target x86_64-unknown-illumos -p forged -p pkgdev --release
```

## Database Migrations

Run migrations manually:

```bash
cargo run -p forged-migration
```

Migrations also run automatically when the server starts.
