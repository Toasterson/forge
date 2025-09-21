#!/usr/bin/env bash
# Wrapper to run cross for x86_64-unknown-illumos using a local sysroot.
#
# This script configures pkg-config and toolchain flags so crates like openssl-sys and
# libarchive-based crates can find headers and libraries in the provided sysroot.
#
# Usage:
#   scripts/cross-illumos-with-sysroot.sh <sysroot.tar.gz|sysroot_dir> [-- <cross args...>]
#
# Examples:
#   scripts/cross-illumos-with-sysroot.sh ./sysroots/illumos-sysroot-omnios-*.tar.gz -- \
#       cross build --target x86_64-unknown-illumos -p forged -p pkgdev
#
# Notes:
# - Cross will mount the repository at /project inside the container. Keep the sysroot
#   under the repository directory so paths match inside the container.
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <sysroot.tar.gz|sysroot_dir> [-- <cross args...>]" >&2
  exit 2
fi

INPUT=$1
shift || true
if [[ "${1:-}" == "--" ]]; then
  shift || true
fi

# Ensure sysroot directory exists inside repo
ROOT_DIR=$(pwd)
WORK_DIR="${ROOT_DIR}/.sysroot"
mkdir -p "${WORK_DIR}"

if [[ -d "${INPUT}" ]]; then
  SYSROOT_DIR="${INPUT}"
elif [[ -f "${INPUT}" ]]; then
  echo "[cross-illumos-with-sysroot] Extracting sysroot tarball: ${INPUT}"
  rm -rf "${WORK_DIR}/current"
  mkdir -p "${WORK_DIR}/current"
  tar -C "${WORK_DIR}/current" -xzf "${INPUT}"
  if [[ -d "${WORK_DIR}/current/sysroot" ]]; then
    SYSROOT_DIR="${WORK_DIR}/current/sysroot"
  else
    SYSROOT_DIR="${WORK_DIR}/current"
  fi
else
  echo "[cross-illumos-with-sysroot] ERROR: input not found: ${INPUT}" >&2
  exit 1
fi

# Resolve to absolute path (as seen both on host and inside cross container at /project)
SYSROOT_DIR=$(python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "${SYSROOT_DIR}")

echo "[cross-illumos-with-sysroot] Using SYSROOT: ${SYSROOT_DIR}"

# Environment for pkg-config and toolchain
export SYSROOT="${SYSROOT_DIR}"
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_SYSROOT_DIR="${SYSROOT_DIR}"
export PKG_CONFIG_LIBDIR="${SYSROOT_DIR}/usr/lib/amd64/pkgconfig:${SYSROOT_DIR}/usr/lib/64/pkgconfig:${SYSROOT_DIR}/usr/lib/pkgconfig:${SYSROOT_DIR}/usr/share/pkgconfig"
export CFLAGS="--sysroot=${SYSROOT_DIR} -I${SYSROOT_DIR}/usr/include"
export CXXFLAGS="${CFLAGS}"
# -R sets runtime library search path for illumos (used by ld on illumos)
export LDFLAGS="--sysroot=${SYSROOT_DIR} -L${SYSROOT_DIR}/usr/lib/amd64 -L${SYSROOT_DIR}/usr/lib/64 -R/usr/lib/amd64 -R/usr/lib/64"
# Help common crates locate OpenSSL and prefer the system copy
export OPENSSL_DIR="${SYSROOT_DIR}/usr"
export OPENSSL_NO_VENDOR=1
# Optional: help libarchive consumers
export LIBARCHIVE_DIR="${SYSROOT_DIR}/usr"

# Export RUSTFLAGS to pass sysroot include/lib hints to build.rs that call cc
export RUSTFLAGS="${RUSTFLAGS:-} -C link-args=--sysroot=${SYSROOT_DIR}"

# Informative echo
echo "[cross-illumos-with-sysroot] Environment configured. Invoking cross..."

# Execute the remainder of arguments, or default to a sensible build
if [[ $# -gt 0 ]]; then
  exec "$@"
else
  exec cross build --target x86_64-unknown-illumos -p forged -p pkgdev
fi
