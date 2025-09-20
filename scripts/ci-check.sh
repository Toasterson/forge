#!/usr/bin/env bash
set -euo pipefail

# Simple local CI helper that mirrors GitHub CI steps
# - Requires: Rust toolchain, cargo
# - Optional: cross (for illumos build)
# - On Debian/Ubuntu, you may need: sudo apt-get install -y protobuf-compiler clang pkg-config libssl-dev libarchive-dev

ROOT_DIR=$(cd "$(dirname "$0")"/.. && pwd)
cd "$ROOT_DIR"

echo "== rustfmt check =="
cargo fmt --all -- --check

echo "== clippy (D warnings) =="
cargo clippy --workspace -- -D warnings

echo "== build (workspace) =="
cargo build --workspace --locked

echo "== test (workspace) =="
cargo test --workspace --locked -- --nocapture

if command -v cross >/dev/null 2>&1; then
  echo "== cross build illumos (debug+release) =="
  cross build --target x86_64-unknown-illumos -p forged -p pkgdev --locked || true
  cross build --target x86_64-unknown-illumos -p forged -p pkgdev --locked --release || true
else
  echo "cross not installed; skipping illumos cross build. Install with: cargo install cross"
fi
