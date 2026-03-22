# Binary Deployment

The simplest deployment method is running the `forged` binary directly.

## Prerequisites

- PostgreSQL 14+
- SeaweedFS
- (Optional) RabbitMQ for build dispatch

## Installation

Download the binary from the [releases page](https://github.com/OpenFlowLabs/forge/releases) or build from source:

```bash
cargo build -p forged --release
cp target/release/forged /usr/local/bin/
```

## Configuration

Create `/etc/forged/forged.toml`:

```toml
[server]
host = "0.0.0.0"
port = 50051

[postgres]
url = "postgresql://forged:forged@localhost/forged"
max_connections = 20

[seaweedfs]
master_url = "http://localhost:9333"

[jj_repos]
path = "/var/lib/forged/repos"
```

## systemd Service

Create `/etc/systemd/system/forged.service`:

```ini
[Unit]
Description=Forge Server
After=network.target postgresql.service

[Service]
Type=simple
User=forged
Group=forged
ExecStart=/usr/local/bin/forged
Environment=FORGED_CONFIG=/etc/forged/forged.toml
Environment=RUST_LOG=forged=info,warn
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now forged
```

## SMF Service (illumos)

For illumos deployments, create an SMF service manifest:

```xml
<?xml version="1.0"?>
<!DOCTYPE service_bundle SYSTEM "/usr/share/lib/xml/dtd/service_bundle.dtd.1">
<service_bundle type="manifest" name="forged">
  <service name="application/forged" type="service" version="1">
    <create_default_instance enabled="true"/>
    <single_instance/>
    <dependency name="network" grouping="require_all" restart_on="error" type="service">
      <service_fmri value="svc:/milestone/network:default"/>
    </dependency>
    <exec_method type="method" name="start"
      exec="/opt/forge/bin/forged"
      timeout_seconds="30">
      <method_context>
        <method_credential user="forged" group="forged"/>
        <method_environment>
          <envvar name="FORGED_CONFIG" value="/etc/forged/forged.toml"/>
          <envvar name="RUST_LOG" value="forged=info,warn"/>
        </method_environment>
      </method_context>
    </exec_method>
    <exec_method type="method" name="stop" exec=":kill" timeout_seconds="30"/>
  </service>
</service_bundle>
```

## Health Check

The server listens on the configured port for gRPC connections. Verify it's running:

```bash
grpcurl -plaintext localhost:50051 list
```
