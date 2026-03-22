# Multi-stage Dockerfile for Forged V2
# Build stage uses current Rust version and leverages Docker layer caching

# Build stage
FROM rust:1.85-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /build

# Copy workspace configuration
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build dependencies first (cached layer)
# This leverages Docker's layer caching for faster rebuilds
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p forged && \
    cp /build/target/release/forged /forged-bin

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates bash \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 -s /bin/bash forged

# Create data directory
RUN mkdir -p /data/jj-repos && chown -R forged:forged /data

# Copy binary from builder
COPY --from=builder /forged-bin /usr/local/bin/forged

# Switch to non-root user
USER forged

# Set working directory
WORKDIR /home/forged

# Expose gRPC port
EXPOSE 50051

# Set default environment variables
ENV FORGED__SERVER__LISTEN_ADDR="0.0.0.0:50051" \
    FORGED__JJ_REPOS__ROOT="/data/jj-repos" \
    RUST_LOG="forged=info"

# Health check: verify gRPC port is accepting connections
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD timeout 5 bash -c "echo > /dev/tcp/localhost/50051" || exit 1

# Run forged
CMD ["/usr/local/bin/forged"]
