# OpenIndiana Setup Guide

This guide walks through a complete production deployment of Forge on OpenIndiana, including all required services: PostgreSQL, SeaweedFS, RabbitMQ, and the Forge server itself with TLS.

## Prerequisites

- OpenIndiana Hipster (2024.04 or later)
- A public IP address (required for ACME/Let's Encrypt)
- A DNS A record pointing your domain to the server (e.g., `forge.example.com`)
- Root or `pfexec` access

## Overview

Forge requires four backing services:

| Service | Purpose | Default Port |
|---|---|---|
| **PostgreSQL** | Metadata storage (actors, gates, components, build jobs) | 5432 |
| **SeaweedFS** | Content-addressable blob storage (archives, patches, artifacts) | 9333, 8080 |
| **RabbitMQ** | Build job dispatch and result consumption via AMQP | 5672 |
| **Forge (forged)** | gRPC server with all application logic | 50051 |

## 1. Install PostgreSQL

OpenIndiana ships PostgreSQL in the repository. The service comes with the data directory already initialized and local password authentication pre-configured:

```bash
pfexec pkg install pkg://openindiana.org/database/postgres-16 \
  pkg://openindiana.org/service/database/postgres-16
pfexec svcadm enable postgresql_16:default
```

### Create the Forge Database and User

```bash
pfexec su - postgres -c "psql -c \"CREATE USER forged WITH PASSWORD 'changeme-use-a-strong-password';\""
pfexec su - postgres -c "psql -c \"CREATE DATABASE forged OWNER forged;\""
pfexec su - postgres -c "psql -c \"GRANT ALL PRIVILEGES ON DATABASE forged TO forged;\""
```

### Verify

```bash
psql -U forged -h localhost -d forged -c "SELECT 1;"
```

## 2. Install SeaweedFS

OpenIndiana provides the SeaweedFS binary as a package:

```bash
pfexec pkg install network/seaweedfs
```

The package installs the `weed` binary but does not include SMF service manifests. You need to create them.

### Create Data Directories and User

```bash
pfexec mkdir -p /var/seaweedfs/master /var/seaweedfs/volume
pfexec groupadd seaweedfs
pfexec useradd -g seaweedfs -d /var/seaweedfs -s /usr/bin/false seaweedfs
pfexec chown -R seaweedfs:seaweedfs /var/seaweedfs
```

### Create SMF Manifests

**SeaweedFS Master** -- save as `/opt/seaweedfs/smf/master.xml`:

```bash
pfexec mkdir -p /opt/seaweedfs/smf
```

```xml
<?xml version="1.0"?>
<!DOCTYPE service_bundle SYSTEM "/usr/share/lib/xml/dtd/service_bundle.dtd.1">
<service_bundle type="manifest" name="seaweedfs-master">
  <service name="network/seaweedfs/master" type="service" version="1">
    <create_default_instance enabled="true"/>
    <single_instance/>
    <dependency name="network" grouping="require_all"
                restart_on="error" type="service">
      <service_fmri value="svc:/milestone/network:default"/>
    </dependency>
    <exec_method type="method" name="start"
      exec="/usr/bin/weed master -mdir=/var/seaweedfs/master -ip=127.0.0.1 -port=9333"
      timeout_seconds="60">
      <method_context>
        <method_credential user="seaweedfs" group="seaweedfs"/>
      </method_context>
    </exec_method>
    <exec_method type="method" name="stop" exec=":kill" timeout_seconds="30"/>
    <property_group name="startd" type="framework">
      <propval name="duration" type="astring" value="child"/>
    </property_group>
    <stability value="Unstable"/>
    <template>
      <common_name><loctext xml:lang="C">SeaweedFS Master</loctext></common_name>
    </template>
  </service>
</service_bundle>
```

**SeaweedFS Volume** -- save as `/opt/seaweedfs/smf/volume.xml`:

```xml
<?xml version="1.0"?>
<!DOCTYPE service_bundle SYSTEM "/usr/share/lib/xml/dtd/service_bundle.dtd.1">
<service_bundle type="manifest" name="seaweedfs-volume">
  <service name="network/seaweedfs/volume" type="service" version="1">
    <create_default_instance enabled="true"/>
    <single_instance/>
    <dependency name="master" grouping="require_all"
                restart_on="error" type="service">
      <service_fmri value="svc:/network/seaweedfs/master:default"/>
    </dependency>
    <exec_method type="method" name="start"
      exec="/usr/bin/weed volume -mserver=127.0.0.1:9333 -port=8080 -dir=/var/seaweedfs/volume -publicUrl=localhost:8080"
      timeout_seconds="60">
      <method_context>
        <method_credential user="seaweedfs" group="seaweedfs"/>
      </method_context>
    </exec_method>
    <exec_method type="method" name="stop" exec=":kill" timeout_seconds="30"/>
    <property_group name="startd" type="framework">
      <propval name="duration" type="astring" value="child"/>
    </property_group>
    <stability value="Unstable"/>
    <template>
      <common_name><loctext xml:lang="C">SeaweedFS Volume Server</loctext></common_name>
    </template>
  </service>
</service_bundle>
```

### Import and Enable

```bash
pfexec svccfg import /opt/seaweedfs/smf/master.xml
pfexec svccfg import /opt/seaweedfs/smf/volume.xml
pfexec svcadm enable seaweedfs/master
pfexec svcadm enable seaweedfs/volume
```

### Verify

```bash
curl http://localhost:9333/cluster/status
```

You should see a JSON response with cluster information.

## 3. Install RabbitMQ

RabbitMQ requires Erlang. On OpenIndiana:

```bash
pfexec pkg install runtime/erlang archiver/gnu-tar
```

If RabbitMQ is not packaged, download and install it. Note: the illumos `tar` silently fails to extract xz archives -- use `gtar` instead:

```bash
# Download RabbitMQ generic Unix package
curl -L -o /tmp/rabbitmq.tar.xz \
  https://github.com/rabbitmq/rabbitmq-server/releases/download/v4.2.5/rabbitmq-server-generic-unix-4.2.5.tar.xz

pfexec mkdir -p /opt/rabbitmq
cd /opt/rabbitmq && pfexec gtar xJf /tmp/rabbitmq.tar.xz --strip-components=1

# RabbitMQ shell scripts use bashisms (local keyword) incompatible with illumos ksh.
# Patch all sbin/ scripts that have a #!/bin/sh shebang to use bash:
for f in /opt/rabbitmq/sbin/*; do
  [ -f "$f" ] && head -1 "$f" | grep -q '^#!/bin/sh' && \
    pfexec sed -i 's|#!/bin/sh|#!/usr/bin/bash|' "$f"
done
```

### Create User and Directories

```bash
pfexec groupadd rabbitmq
pfexec useradd -g rabbitmq -d /var/rabbitmq -s /usr/bin/false rabbitmq
pfexec mkdir -p /var/rabbitmq /var/log/rabbitmq
pfexec chown rabbitmq:rabbitmq /var/rabbitmq /var/log/rabbitmq
```

### Create SMF Manifest

`/opt/rabbitmq/smf/rabbitmq.xml`:

```xml
<?xml version="1.0"?>
<!DOCTYPE service_bundle SYSTEM "/usr/share/lib/xml/dtd/service_bundle.dtd.1">
<service_bundle type="manifest" name="rabbitmq">
  <service name="application/rabbitmq" type="service" version="1">
    <create_default_instance enabled="true"/>
    <single_instance/>
    <dependency name="network" grouping="require_all"
                restart_on="error" type="service">
      <service_fmri value="svc:/milestone/network:default"/>
    </dependency>
    <exec_method type="method" name="start"
      exec="/opt/rabbitmq/sbin/rabbitmq-server"
      timeout_seconds="60">
      <method_context>
        <method_credential user="rabbitmq" group="rabbitmq"/>
        <method_environment>
          <envvar name="RABBITMQ_MNESIA_BASE" value="/var/rabbitmq"/>
          <envvar name="RABBITMQ_LOG_BASE" value="/var/log/rabbitmq"/>
          <envvar name="HOME" value="/var/rabbitmq"/>
        </method_environment>
      </method_context>
    </exec_method>
    <exec_method type="method" name="stop" exec=":kill" timeout_seconds="30"/>
    <property_group name="startd" type="framework">
      <propval name="duration" type="astring" value="child"/>
    </property_group>
    <stability value="Unstable"/>
    <template>
      <common_name><loctext xml:lang="C">RabbitMQ Message Broker</loctext></common_name>
    </template>
  </service>
</service_bundle>
```

### Import, Enable, and Configure

```bash
pfexec svccfg import /opt/rabbitmq/smf/rabbitmq.xml
pfexec svcadm enable rabbitmq

# Wait for RabbitMQ to start, then configure
sleep 10

# Bind RabbitMQ to localhost only (not exposed to the network)
pfexec mkdir -p /opt/rabbitmq/etc/rabbitmq
cat <<'CONF' | pfexec tee /opt/rabbitmq/etc/rabbitmq/rabbitmq.conf
listeners.tcp.local = 127.0.0.1:5672
CONF
pfexec svcadm restart rabbitmq
sleep 5

# rabbitmqctl must read the Erlang cookie from the rabbitmq user's home directory.
# Set HOME so it finds the right cookie:
export RABBITMQ_HOME=/var/rabbitmq

# Enable management plugin (optional, for web UI on port 15672)
HOME=$RABBITMQ_HOME pfexec /opt/rabbitmq/sbin/rabbitmq-plugins enable rabbitmq_management

# Create vhost and user for Forge
HOME=$RABBITMQ_HOME pfexec /opt/rabbitmq/sbin/rabbitmqctl add_vhost master
HOME=$RABBITMQ_HOME pfexec /opt/rabbitmq/sbin/rabbitmqctl add_user forged changeme-use-a-strong-password
HOME=$RABBITMQ_HOME pfexec /opt/rabbitmq/sbin/rabbitmqctl set_permissions -p master forged ".*" ".*" ".*"
```

### Verify

```bash
HOME=/var/rabbitmq pfexec /opt/rabbitmq/sbin/rabbitmqctl status
```

## 4. Install Forge

### Create User and Directories

```bash
pfexec groupadd forged
pfexec useradd -g forged -d /opt/forge -s /usr/bin/false forged
pfexec mkdir -p /opt/forge/bin /opt/forge/lib/svc/method
pfexec mkdir -p /etc/forged
pfexec mkdir -p /var/lib/forged/jj-repos /var/lib/forged/acme
pfexec chown -R forged:forged /var/lib/forged
```

### Install the Binaries

If you have pre-built illumos binaries:

```bash
pfexec cp forged /opt/forge/bin/
pfexec cp pkgdev /opt/forge/bin/
pfexec chmod +x /opt/forge/bin/*
```

If building from source on the machine:

```bash
# Install build dependencies
pfexec pkg install developer/gcc-14 developer/build/gnu-make \
  library/security/openssl-31 system/header \
  system/library/gcc-14-runtime system/library/g++-14-runtime \
  developer/build/pkg-config library/libarchive \
  developer/linker developer/object-file \
  library/c++/protobuf \
  developer/lang/rustc

# Build
cargo build -p forged -p pkgdev --release
pfexec cp target/release/forged target/release/pkgdev /opt/forge/bin/
```

### Install the SMF Manifest and Method Script

```bash
pfexec cp smf/forged.xml /lib/svc/manifest/application/forge-forged.xml
pfexec cp smf/forged-method /opt/forge/lib/svc/method/forged-method
pfexec chmod +x /opt/forge/lib/svc/method/forged-method
```

### Write the Configuration File

Create `/etc/forged/forged.toml`:

```toml
[server]
# Use port 443 for public TLS servers so clients don't need to specify a port.
# Use 50051 for internal/development deployments.
listen_addr = "0.0.0.0:443"

[postgres]
url = "postgresql://forged:changeme-use-a-strong-password@localhost/forged"
max_connections = 20

[seaweedfs]
master_url = "http://localhost:9333"
namespace = "default"
connect_timeout_secs = 5
request_timeout_secs = 30
max_retries = 3

[jj_repos]
root = "/var/lib/forged/jj-repos"

[amqp]
url = "amqp://forged:changeme-use-a-strong-password@localhost:5672/master"

[tls]
mode = "acme"

[tls.acme]
domains = ["forge.example.com"]
contact = ["mailto:admin@example.com"]
cache_dir = "/var/lib/forged/acme"
challenge_type = "http-01"
http_listen_addr = "0.0.0.0:80"
```

Set file permissions:

```bash
pfexec chown root:forged /etc/forged/forged.toml
pfexec chmod 640 /etc/forged/forged.toml
```

### Import and Enable the Service

```bash
pfexec svccfg import /lib/svc/manifest/application/forge-forged.xml
pfexec svcadm enable forge/forged
```

### Verify

```bash
svcs forge/forged
# Should show: online

# Check logs if there are issues
svcs -xv forge/forged
tail -f $(svcs -L forge/forged)
```

## 5. TLS with Let's Encrypt

When `tls.mode = "acme"` is set, Forge automatically obtains a TLS certificate from Let's Encrypt on first startup.

### Requirements

- Port 80 must be open to the internet for HTTP-01 challenge verification
- DNS must resolve `forge.example.com` to this server's public IP
- The `forged` user must be able to bind port 80

### Privileged Ports

The server listens on port 443 (gRPC/TLS) and port 80 (ACME HTTP-01), both below 1024. The SMF manifest grants the `net_privaddr` privilege to the `forged` user via `method_credential`, so no additional configuration is needed.

### Certificate Lifecycle

- Certificates are cached in `/var/lib/forged/acme/`
- Automatic renewal checks run every 12 hours
- Renewal happens 30 days before expiry
- Account credentials are persisted across restarts

### Using Manual Certificates Instead

If you prefer to manage certificates yourself (e.g., behind a reverse proxy):

```toml
[tls]
mode = "manual"

[tls.manual]
cert_file = "/etc/ssl/certs/forge.pem"
key_file = "/etc/ssl/private/forge.key"
```

## 6. Firewall Configuration

Open the required ports using `ipfilter`:

```bash
# /etc/ipf/ipf.conf
# Allow gRPC over TLS (port 443 for public servers, or 50051 if using a non-standard port)
pass in on e1000g0 proto tcp from any to any port = 443

# Allow HTTP for ACME challenges
pass in on e1000g0 proto tcp from any to any port = 80

# Allow SSH (for administration)
pass in on e1000g0 proto tcp from any to any port = 22

# Block everything else from outside
block in on e1000g0 all
```

Apply:

```bash
pfexec svcadm enable ipfilter
pfexec ipf -Fa -f /etc/ipf/ipf.conf
```

Internal services (PostgreSQL 5432, SeaweedFS 9333/8080, RabbitMQ 5672) should only be accessible from localhost. The default configuration binds them to `127.0.0.1`, which is already secure.

## 7. Verify the Full Stack

Check all services are running:

```bash
svcs -a | grep -E 'postgres|seaweedfs|rabbitmq|forge'
```

Expected output:

```
online  svc:/application/database/postgresql_16:default
online  svc:/network/seaweedfs/master:default
online  svc:/network/seaweedfs/volume:default
online  svc:/application/rabbitmq:default
online  svc:/application/forge/forged:default
```

Test the gRPC endpoint:

```bash
# Without TLS (development, port 50051)
grpcurl -plaintext localhost:50051 grpc.health.v1.Health/Check

# With TLS on port 443
grpcurl forge.example.com:443 grpc.health.v1.Health/Check
```

Expected response:

```json
{
  "status": "SERVING"
}
```

## 8. Post-Installation

### Run Database Migrations

Migrations run automatically on server startup. To check status manually:

```bash
/opt/forge/bin/forged admin migrate-status
```

### Set Up Backups

Use the provided backup script:

```bash
pfexec cp scripts/backup.sh /opt/forge/bin/
pfexec chmod +x /opt/forge/bin/backup.sh

# Run a backup
pfexec su - forged -c 'FORGED_POSTGRES_URL="postgresql://forged:password@localhost/forged" /opt/forge/bin/backup.sh /var/backups/forged'
```

Schedule daily backups with cron:

```bash
pfexec crontab -e -u forged
# Add:
0 2 * * * FORGED_POSTGRES_URL="postgresql://forged:password@localhost/forged" /opt/forge/bin/backup.sh /var/backups/forged/$(date +\%Y\%m\%d)
```

### Configure Logging

Forge logs to the SMF log facility. View logs:

```bash
# Follow live logs
tail -f $(svcs -L forge/forged)

# Increase verbosity temporarily
svccfg -s forge/forged setenv RUST_LOG forged=debug
svcadm refresh forge/forged
svcadm restart forge/forged
```

### Register Your First User

From a client machine with `pkgdev` installed:

```bash
pkgdev auth login --host https://forge.example.com
pkgdev auth add-key --public-key ~/.ssh/id_ed25519.pub --key-id default
```

## 9. Troubleshooting

### Service Won't Start

```bash
# Check service status
svcs -xv forge/forged

# View the log
tail -100 $(svcs -L forge/forged)
```

Common issues:

- **"PostgreSQL URL is required"** -- Check `/etc/forged/forged.toml` exists and is readable by the `forged` user
- **"Failed to connect to PostgreSQL"** -- Verify PostgreSQL is running (`svcs postgresql`) and the password is correct
- **"Failed to bind"** -- Check if the port is already in use (`netstat -an | grep 443`) or if the `forged` user lacks the `net_privaddr` privilege
- **"ACME certificate not found"** -- The ACME flow may have failed. Check that port 80 is reachable and DNS resolves correctly

### SeaweedFS Health Check Failing

```bash
curl http://localhost:9333/cluster/status
# If no response, check:
svcs -xv seaweedfs/master
svcs -xv seaweedfs/volume
```

### RabbitMQ Connection Refused

```bash
HOME=/var/rabbitmq pfexec /opt/rabbitmq/sbin/rabbitmqctl status
# Check vhost exists:
HOME=/var/rabbitmq pfexec /opt/rabbitmq/sbin/rabbitmqctl list_vhosts
# Check user permissions:
HOME=/var/rabbitmq pfexec /opt/rabbitmq/sbin/rabbitmqctl list_permissions -p master
```

### ACME Certificate Issues

```bash
# Check if port 80 is reachable from outside
curl http://forge.example.com/.well-known/acme-challenge/test

# Check cached certificates
ls -la /var/lib/forged/acme/

# Force certificate renewal by removing cache
pfexec rm /var/lib/forged/acme/cert.pem /var/lib/forged/acme/key.pem
pfexec svcadm restart forge/forged
```

## 10. Upgrading

```bash
# Stop the service
pfexec svcadm disable forge/forged

# Replace binaries
pfexec cp new-forged /opt/forge/bin/forged
pfexec cp new-pkgdev /opt/forge/bin/pkgdev

# Start the service (migrations run automatically)
pfexec svcadm enable forge/forged

# Verify
svcs forge/forged
```
