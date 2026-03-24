# Multi-stage build for forged server container
# Builder stage
FROM rust:1.86-bookworm AS builder

# Install build dependencies (protoc for tonic/prost, clang for rocksdb-sys, OpenSSL)
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev libarchive-dev protobuf-compiler clang \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace manifests and source
COPY Cargo.lock Cargo.toml ./
COPY src ./src
COPY crates ./crates

# Build only the server binary (release)
RUN cargo build -p forged --release

# Runtime stage
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates bash \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -u 10001 -m forged

WORKDIR /app
COPY --from=builder /app/target/release/forged /usr/local/bin/forged

# Default data directory
VOLUME ["/data"]
ENV RUST_LOG=info \
    FORGED__SERVER__LISTEN_ADDR=0.0.0.0:50051 \
    FORGED__POSTGRES__URL=postgresql://forged:forged@localhost/forged \
    FORGED__JJ_REPOS__ROOT=/data/jj-repos

EXPOSE 50051

# Health check: verify gRPC port is accepting connections
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
  CMD timeout 5 bash -c "echo > /dev/tcp/localhost/50051" || exit 1

USER forged
ENTRYPOINT ["/usr/local/bin/forged"]
