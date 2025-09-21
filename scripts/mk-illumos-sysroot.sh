#!/usr/bin/env bash
# Build an illumos sysroot tarball containing OpenSSL and libarchive using libips (Linux-native).
#
# This wrapper delegates to `cargo run -p xtask -- sysroot` which uses the Rust libips crate to
# download and install packages from an OmniOS IPS repository into a local sysroot.
#
# Requirements:
# - Rust toolchain and cargo
# - Network access to the OmniOS repository
#
# Usage examples:
#   scripts/mk-illumos-sysroot.sh                                   # default repo + packages
#   OMNIOS_REPO_URL=https://pkg.omnios.org/r151052/core \
#     PACKAGES="library/security/openssl library/libarchive developer/build/pkg-config" \
#     scripts/mk-illumos-sysroot.sh
#   OUT_DIR=./sysroots scripts/mk-illumos-sysroot.sh
set -euo pipefail

OMNIOS_REPO_URL=${OMNIOS_REPO_URL:-"https://pkg.omnios.org/r151052/core"}
PUBLISHER=${PUBLISHER:-"omnios"}
PACKAGES=${PACKAGES:-"library/security/openssl library/libarchive developer/pkg-config"}
OUT_DIR=${OUT_DIR:-"$(pwd)/sysroots"}
NAME=${NAME:-"omnios-r151052"}

# Normalize packages into array
# shellcheck disable=SC2206
PKGS=(${PACKAGES})

echo "[mk-illumos-sysroot] Using libips via cargo xtask to build sysroot"
echo "[mk-illumos-sysroot] Repo: ${OMNIOS_REPO_URL}"
echo "[mk-illumos-sysroot] Publisher: ${PUBLISHER}"
echo "[mk-illumos-sysroot] Packages: ${PACKAGES}"
echo "[mk-illumos-sysroot] Out dir: ${OUT_DIR}"

cargo run -p xtask -- sysroot \
  --repo "${OMNIOS_REPO_URL}" \
  --publisher "${PUBLISHER}" \
  --packages "${PKGS[@]}" \
  --out-dir "${OUT_DIR}" \
  --name "${NAME}"
