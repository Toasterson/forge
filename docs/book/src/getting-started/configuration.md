# Configuration

Forge loads configuration in the following order of precedence (highest first):

1. Environment variables (`FORGED__*` with `__` as separator)
2. `./forged.toml` in the working directory
3. Path specified by the `FORGED_CONFIG` environment variable
4. Built-in defaults

## Configuration File

Create a `forged.toml` file:

```toml
[server]
host = "0.0.0.0"
port = 50051

[postgres]
url = "postgresql://forged:forged@localhost/forged"
max_connections = 10

[seaweedfs]
master_url = "http://localhost:9333"

[jj_repos]
path = "/var/lib/forged/repos"

[oidc]
issuer_url = "https://auth.example.com"
client_id = "forged"
```

## Environment Variables

All configuration values can be set via environment variables using the `FORGED__` prefix with `__` as a section separator:

| Variable | Description | Default |
|---|---|---|
| `FORGED__SERVER__HOST` | Listen address | `0.0.0.0` |
| `FORGED__SERVER__PORT` | Listen port | `50051` |
| `FORGED__POSTGRES__URL` | PostgreSQL connection string | `postgresql://forged:forged@localhost/forged` |
| `FORGED__SEAWEEDFS__MASTER_URL` | SeaweedFS master URL | `http://localhost:9333` |
| `FORGED__JJ_REPOS__PATH` | Jujutsu repository storage path | `./repos` |

## Logging

Forge uses the `tracing` crate. Control log verbosity with the `RUST_LOG` environment variable:

```bash
# Show info-level logs for forge, warn for everything else
RUST_LOG=forged=info,warn cargo run -p forged

# Debug logging for specific modules
RUST_LOG=forged::services=debug,forged::transport=trace cargo run -p forged
```
