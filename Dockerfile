# Multi-stage build for forged server container
# Builder stage
FROM rust:1.79-bullseye AS builder

# Install build dependencies (protoc for tonic/prost, clang for rocksdb-sys, OpenSSL)
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev protobuf-compiler clang \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies
COPY Cargo.lock Cargo.toml ./
COPY crates ./crates
COPY forged.toml ./forged.toml

# Build only the server binary (release)
RUN cargo build -p forged --release

# Runtime stage
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -u 10001 -m forged

WORKDIR /app
COPY --from=builder /app/target/release/forged /usr/local/bin/forged

# Default data directory
VOLUME ["/data"]
ENV RUST_LOG=info \
    FORGED__SERVER__LISTEN_ADDR=0.0.0.0:50051 \
    FORGED__SURREAL__MODE=embedded \
    FORGED__SURREAL__PATH=/data/surreal \
    FORGED__REPOS__MODE=fs \
    FORGED__REPOS__ROOT=/data/repos

EXPOSE 50051

USER forged
ENTRYPOINT ["/usr/local/bin/forged"]
