# Storage

Forge uses multiple storage backends, each optimized for its role.

## PostgreSQL

Stores all structured metadata:

- **Actors** -- user accounts, public keys, roles
- **Gates** -- gate definitions, membership
- **Components** -- component recipes, file references
- **Build jobs** -- job status, logs, results
- **Blob metadata** -- SHA-256 hashes, sizes, locations

Access is through SeaORM entities and repositories. Database migrations are managed by the `forged-migration` crate.

### Running Migrations

```bash
cargo run -p forged-migration
```

Migrations run automatically on server startup.

## SeaweedFS

Content-addressable blob storage for large binary data:

- Source archives (tarballs, zip files)
- Patch files
- License files
- Build scripts
- Build artifacts

Files are stored by their SHA-256 hash, providing deduplication and integrity verification. The SeaweedFS master runs on port 9333 by default.

### How It Works

1. Client uploads a file via `UploadSourceArchive` or `UploadComponentFile`
2. Forge computes the SHA-256 hash
3. The file is stored in SeaweedFS with the hash as the key
4. Metadata (hash, size, filename) is recorded in PostgreSQL
5. Downloads are served by streaming from SeaweedFS

## Jujutsu

Forge uses [Jujutsu](https://martinvonz.github.io/jj/) as the version control backend with a custom storage adapter that stores repository data in SeaweedFS instead of the local filesystem. This enables distributed repository storage without requiring shared filesystem access.

## RabbitMQ

Message queue for asynchronous build operations:

- **Build dispatch** -- the `BuildDispatchService` publishes build jobs to a queue consumed by Solstice CI workers
- **Build reports** -- the `BuildReportConsumer` listens for build completion messages and updates job status

Connection is managed via `deadpool-lapin` (AMQP connection pool).

## Development Setup

Start all required storage services:

```bash
docker compose -f docker-compose.dev.yml up -d
```

This starts:

- PostgreSQL on port 5432
- SeaweedFS master on port 9333
- SeaweedFS volume server on port 8080

Key environment variables:

| Variable | Default |
|---|---|
| `TEST_DATABASE_URL` | `postgresql://forged:forged@localhost/forged_test` |
| `TEST_SEAWEEDFS_URL` | `http://localhost:9333` |
