# Docker Deployment

## Single Container

Run the Forge server with Docker:

```bash
docker run -d \
  --name forged \
  -p 50051:50051 \
  -e FORGED__POSTGRES__URL=postgresql://forged:forged@host.docker.internal/forged \
  -e FORGED__SEAWEEDFS__MASTER_URL=http://host.docker.internal:9333 \
  ghcr.io/openflowlabs/forged:latest
```

## Docker Compose

For a complete stack including all dependencies:

```yaml
version: "3.8"

services:
  postgres:
    image: postgres:16
    environment:
      POSTGRES_USER: forged
      POSTGRES_PASSWORD: forged
      POSTGRES_DB: forged
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data

  seaweedfs-master:
    image: chrislusf/seaweedfs
    command: master
    ports:
      - "9333:9333"

  seaweedfs-volume:
    image: chrislusf/seaweedfs
    command: volume -mserver=seaweedfs-master:9333 -port=8080
    ports:
      - "8080:8080"
    depends_on:
      - seaweedfs-master

  forged:
    image: ghcr.io/openflowlabs/forged:latest
    ports:
      - "50051:50051"
    environment:
      FORGED__POSTGRES__URL: postgresql://forged:forged@postgres/forged
      FORGED__SEAWEEDFS__MASTER_URL: http://seaweedfs-master:9333
      RUST_LOG: forged=info,warn
    depends_on:
      - postgres
      - seaweedfs-master
      - seaweedfs-volume

volumes:
  pgdata:
```

Start the stack:

```bash
docker compose up -d
```

## Environment Variables

All configuration can be set via environment variables with the `FORGED__` prefix:

| Variable | Description |
|---|---|
| `FORGED__SERVER__HOST` | Listen address (default: `0.0.0.0`) |
| `FORGED__SERVER__PORT` | Listen port (default: `50051`) |
| `FORGED__POSTGRES__URL` | PostgreSQL connection string |
| `FORGED__SEAWEEDFS__MASTER_URL` | SeaweedFS master URL |
| `RUST_LOG` | Log level configuration |

## Optional Features

### OpenTelemetry

Build the image with the `otel` feature to enable trace and metric export:

```dockerfile
RUN cargo build -p forged --release --features otel
```

### QUIC Transport

Build with the `quic` feature for a QUIC endpoint alongside gRPC:

```dockerfile
RUN cargo build -p forged --release --features quic
```
