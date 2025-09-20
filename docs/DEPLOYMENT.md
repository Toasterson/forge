# Deployment guide

This guide walks through common ways to deploy the `forged` server:
- Single binary on a VM or bare-metal (with a systemd unit)
- Docker container
- Docker Compose
- Kubernetes with Helm (charts/forged)

It also covers configuration, persistence, logging, and optional features.

## Prerequisites

- A TCP port available for gRPC (default 50051)
- Optional data directory for embedded storage (defaults shown below)
- For Kubernetes: `helm` and access to a cluster

## Configuration primer

The server reads configuration in this order:
1. If `FORGED_CONFIG` is set, load that file (errors if it doesn’t exist).
2. Else, if `./forged.toml` exists in the current working directory, load it.
3. Apply environment overrides (prefix `FORGED__`, `__` separates nested fields) and defaults.

Additional address fallback: `FORGED_ADDR` may override `server.listen_addr` just before parsing.

Useful environment variables:

```
# Network
FORGED__SERVER__LISTEN_ADDR=0.0.0.0:50051

# SurrealDB storage
FORGED__SURREAL__MODE=embedded            # or "clustered"
FORGED__SURREAL__PATH=/data/surreal       # when embedded
FORGED__SURREAL__ENDPOINT=ws://127.0.0.1:8000  # when clustered
FORGED__SURREAL__USERNAME=root
FORGED__SURREAL__PASSWORD=secret
FORGED__SURREAL__NAMESPACE=forged
FORGED__SURREAL__DATABASE=default

# Git repository object storage
FORGED__REPOS__MODE=fs                    # or "s3"
FORGED__REPOS__ROOT=/data/repos           # when mode=fs

# SMTP (optional)
FORGED__SMTP__HOST=smtp.example.com
FORGED__SMTP__PORT=587
FORGED__SMTP__USERNAME=forge@example.com
FORGED__SMTP__PASSWORD=...
FORGED__SMTP__FROM=forge@example.com
FORGED__SMTP__STARTTLS=true
```

Minimal `forged.toml` example:

```toml
[server]
listen_addr = "0.0.0.0:50051"

[surreal]
mode = "embedded"
path = "./data/surreal"

[repos]
mode = "fs"
root = "./data/repos"
```

## 1) Single binary deployment

### Download the binary

- Go to this repository’s Releases page and download the asset for your OS/arch.
  - In GitHub UI: open this repository ➜ “Releases” in the sidebar.
  - Or open the Releases page for this repository’s URL.
- Unpack the archive to `/usr/local/bin/forged` or another suitable location.

### Create directories

```
sudo mkdir -p /var/lib/forged/surreal /var/lib/forged/repos
sudo useradd --system --home /var/lib/forged --shell /sbin/nologin forged 2>/dev/null || true
sudo chown -R forged:forged /var/lib/forged
```

### Systemd service unit (Linux)

Save as `/etc/systemd/system/forged.service`:

```
[Unit]
Description=Forged server
After=network-online.target
Wants=network-online.target

[Service]
User=forged
Group=forged
WorkingDirectory=/var/lib/forged
Environment=FORGED__SERVER__LISTEN_ADDR=0.0.0.0:50051
Environment=FORGED__SURREAL__MODE=embedded
Environment=FORGED__SURREAL__PATH=/var/lib/forged/surreal
Environment=FORGED__REPOS__MODE=fs
Environment=FORGED__REPOS__ROOT=/var/lib/forged/repos
# Optional SMTP, QUIC, OTEL
# Environment=FORGED_QUIC_ADDR=0.0.0.0:50052
# Environment=OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317
ExecStart=/usr/local/bin/forged
Restart=on-failure
RestartSec=3s

[Install]
WantedBy=multi-user.target
```

Reload and start:

```
sudo systemctl daemon-reload
sudo systemctl enable --now forged
journalctl -u forged -f
```

## 2) Docker

Use the published image if available, or build locally.

### Pull and run

```
docker run -d --name forged \
  -p 50051:50051 \
  -e FORGED__SERVER__LISTEN_ADDR=0.0.0.0:50051 \
  -e FORGED__SURREAL__MODE=embedded \
  -e FORGED__SURREAL__PATH=/data/surreal \
  -e FORGED__REPOS__MODE=fs \
  -e FORGED__REPOS__ROOT=/data/repos \
  -v $(pwd)/data:/data \
  ghcr.io/toasterson/forge:latest
```

### Build locally (from repo root)

```
docker build -t forged:dev .
docker run -d --name forged -p 50051:50051 -v $(pwd)/data:/data forged:dev
```

## 3) Docker Compose

The repository includes a `docker-compose.dev.yml` that provides handy supporting services (RabbitMQ, Postgres, MinIO) for development. To bring those up:

```
docker compose -f docker-compose.dev.yml up -d
```

To run `forged` alongside them, create a minimal `docker-compose.yml` in your project with a `forged` service:

```yaml
version: "3.9"
services:
  forged:
    image: ghcr.io/toasterson/forge:latest
    ports:
      - "50051:50051"
    environment:
      FORGED__SERVER__LISTEN_ADDR: 0.0.0.0:50051
      FORGED__SURREAL__MODE: embedded
      FORGED__SURREAL__PATH: /data/surreal
      FORGED__REPOS__MODE: fs
      FORGED__REPOS__ROOT: /data/repos
    volumes:
      - ./data:/data
```

Then run:

```
docker compose up -d
```

## 4) Kubernetes (Helm)

A Helm chart is provided under `charts/forged`. Example installation:

```
# from the repository root
helm upgrade --install forged ./charts/forged \
  --namespace forged --create-namespace \
  --set image.repository=ghcr.io/toasterson/forge \
  --set image.tag=latest \
  --set service.type=ClusterIP \
  --set env.FORGED__SERVER__LISTEN_ADDR=0.0.0.0:50051 \
  --set env.FORGED__SURREAL__MODE=embedded \
  --set env.FORGED__SURREAL__PATH=/data/surreal \
  --set env.FORGED__REPOS__MODE=fs \
  --set env.FORGED__REPOS__ROOT=/data/repos
```

Persistence: by default the chart uses an `emptyDir` at `/data`. To enable a PVC with 5Gi default size:

```
helm upgrade --install forged ./charts/forged \
  --namespace forged --create-namespace \
  --set persistence.enabled=true \
  --set persistence.size=20Gi \
  --set persistence.storageClassName=standard
```

Service type: to expose outside the cluster, set `service.type=LoadBalancer` or create an Ingress as appropriate.

### Environment values in the chart

Default env in `values.yaml`:

```
FORGED__SERVER__LISTEN_ADDR=0.0.0.0:50051
FORGED__SURREAL__MODE=embedded
FORGED__SURREAL__PATH=/data/surreal
FORGED__REPOS__MODE=fs
FORGED__REPOS__ROOT=/data/repos
```

## Optional features

- QUIC endpoint stub: build with `--features quic` and set `FORGED_QUIC_ADDR` at runtime (e.g., `0.0.0.0:50052`).
- OpenTelemetry export: build with `--features otel` and set `OTEL_EXPORTER_OTLP_ENDPOINT`.

## Logging

Logging is provided by the `tracing` crate with `tracing-subscriber::EnvFilter`.
- Set `RUST_LOG` to control verbosity, e.g.: `RUST_LOG=forged=debug,forged::services=trace`.
- Default level is `info` if `RUST_LOG` is not set.

## Health and ports

- gRPC service: TCP on the configured `listen_addr` (default 50051)
- The Helm chart defines a TCP liveness/readiness probe on that port.

## Upgrades

- Replace the binary (or update the container image), keep your data directory.
- For Helm, use `helm upgrade` with a new image tag.

## Uninstall

- systemd: `systemctl disable --now forged && rm -f /etc/systemd/system/forged.service`
- Docker: `docker rm -f forged`
- Helm: `helm uninstall forged -n forged`
