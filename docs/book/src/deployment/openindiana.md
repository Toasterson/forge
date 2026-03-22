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

OpenIndiana ships PostgreSQL in the repository:

```bash
sudo pkg install database/postgres-17
```

### Initialize the Database

```bash
# Initialize the data directory
sudo -u postgres /usr/postgres/17/bin/initdb -D /var/postgres/17/data

# Enable and start the service
sudo svcadm enable postgresql:version_17
```

### Create the Forge Database and User

```bash
sudo -u postgres psql <<SQL
CREATE USER forged WITH PASSWORD 'changeme-use-a-strong-password';
CREATE DATABASE forged OWNER forged;
GRANT ALL PRIVILEGES ON DATABASE forged TO forged;
SQL
```

### Configure PostgreSQL for Local Connections

Edit `/var/postgres/17/data/pg_hba.conf` to allow password authentication for the `forged` user:

```
# TYPE  DATABASE  USER   ADDRESS        METHOD
host    forged    forged 127.0.0.1/32   scram-sha-256
host    forged    forged ::1/128        scram-sha-256
```

Reload the configuration:

```bash
sudo svcadm refresh postgresql:version_17
```

### Verify

```bash
psql -U forged -h localhost -d forged -c "SELECT 1;"
```

## 2. Install SeaweedFS

SeaweedFS is not packaged for OpenIndiana. Download the latest release binary:

```bash
# Download SeaweedFS
curl -L -o /tmp/seaweedfs.tar.gz \
  https://github.com/seaweedfs/seaweedfs/releases/latest/download/linux_amd64.tar.gz

# Extract (SeaweedFS provides a single binary called 'weed')
sudo mkdir -p /opt/seaweedfs/bin
cd /tmp && tar xzf seaweedfs.tar.gz
sudo cp weed /opt/seaweedfs/bin/
sudo chmod +x /opt/seaweedfs/bin/weed
```

> **Note**: SeaweedFS provides illumos/Solaris binaries for some releases. Check the releases page for `solaris_amd64` builds. If unavailable, build from source with Go or use a Linux binary under an lx-branded zone.

### Create Data Directories

```bash
sudo mkdir -p /var/seaweedfs/master /var/seaweedfs/volume
sudo useradd -d /var/seaweedfs -s /usr/bin/false seaweedfs
sudo chown -R seaweedfs:seaweedfs /var/seaweedfs
```

### Create SMF Manifests

**SeaweedFS Master** -- `/opt/seaweedfs/smf/master.xml`:

```xml
<?xml version="1.0"?>
<!DOCTYPE service_bundle SYSTEM "/usr/share/lib/xml/dtd/service_bundle.dtd.1">
<service_bundle type="manifest" name="seaweedfs-master">
  <service name="application/seaweedfs/master" type="service" version="1">
    <create_default_instance enabled="true"/>
    <single_instance/>
    <dependency name="network" grouping="require_all"
                restart_on="error" type="service">
      <service_fmri value="svc:/milestone/network:default"/>
    </dependency>
    <exec_method type="method" name="start"
      exec="/opt/seaweedfs/bin/weed master -mdir=/var/seaweedfs/master -ip=127.0.0.1 -port=9333"
      timeout_seconds="30">
      <method_context>
        <method_credential user="seaweedfs" group="seaweedfs"/>
      </method_context>
    </exec_method>
    <exec_method type="method" name="stop" exec=":kill" timeout_seconds="30"/>
    <stability value="Unstable"/>
    <template>
      <common_name><loctext xml:lang="C">SeaweedFS Master</loctext></common_name>
    </template>
  </service>
</service_bundle>
```

**SeaweedFS Volume** -- `/opt/seaweedfs/smf/volume.xml`:

```xml
<?xml version="1.0"?>
<!DOCTYPE service_bundle SYSTEM "/usr/share/lib/xml/dtd/service_bundle.dtd.1">
<service_bundle type="manifest" name="seaweedfs-volume">
  <service name="application/seaweedfs/volume" type="service" version="1">
    <create_default_instance enabled="true"/>
    <single_instance/>
    <dependency name="master" grouping="require_all"
                restart_on="error" type="service">
      <service_fmri value="svc:/application/seaweedfs/master:default"/>
    </dependency>
    <exec_method type="method" name="start"
      exec="/opt/seaweedfs/bin/weed volume -mserver=127.0.0.1:9333 -port=8080 -dir=/var/seaweedfs/volume"
      timeout_seconds="30">
      <method_context>
        <method_credential user="seaweedfs" group="seaweedfs"/>
      </method_context>
    </exec_method>
    <exec_method type="method" name="stop" exec=":kill" timeout_seconds="30"/>
    <stability value="Unstable"/>
    <template>
      <common_name><loctext xml:lang="C">SeaweedFS Volume Server</loctext></common_name>
    </template>
  </service>
</service_bundle>
```

### Import and Enable

```bash
sudo svccfg import /opt/seaweedfs/smf/master.xml
sudo svccfg import /opt/seaweedfs/smf/volume.xml
sudo svcadm enable seaweedfs/master
sudo svcadm enable seaweedfs/volume
```

### Verify

```bash
curl http://localhost:9333/cluster/status
```

You should see a JSON response with cluster information.

## 3. Install RabbitMQ

RabbitMQ requires Erlang. On OpenIndiana:

```bash
sudo pkg install runtime/erlang
```

If RabbitMQ is not packaged, download and install it:

```bash
# Download RabbitMQ generic Unix package
curl -L -o /tmp/rabbitmq.tar.xz \
  https://github.com/rabbitmq/rabbitmq-server/releases/download/v3.12.14/rabbitmq-server-generic-unix-3.12.14.tar.xz

sudo mkdir -p /opt/rabbitmq
cd /opt/rabbitmq && sudo tar xJf /tmp/rabbitmq.tar.xz --strip-components=1
```

### Create User and Directories

```bash
sudo useradd -d /var/rabbitmq -s /usr/bin/false rabbitmq
sudo mkdir -p /var/rabbitmq
sudo chown rabbitmq:rabbitmq /var/rabbitmq
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
    <exec_method type="method" name="stop"
      exec="/opt/rabbitmq/sbin/rabbitmqctl stop"
      timeout_seconds="30">
      <method_context>
        <method_credential user="rabbitmq" group="rabbitmq"/>
        <method_environment>
          <envvar name="HOME" value="/var/rabbitmq"/>
        </method_environment>
      </method_context>
    </exec_method>
    <stability value="Unstable"/>
    <template>
      <common_name><loctext xml:lang="C">RabbitMQ Message Broker</loctext></common_name>
    </template>
  </service>
</service_bundle>
```

### Import, Enable, and Configure

```bash
sudo mkdir -p /var/log/rabbitmq
sudo chown rabbitmq:rabbitmq /var/log/rabbitmq

sudo svccfg import /opt/rabbitmq/smf/rabbitmq.xml
sudo svcadm enable rabbitmq

# Wait for RabbitMQ to start, then configure
sleep 10

# Enable management plugin (optional, for web UI on port 15672)
sudo -u rabbitmq /opt/rabbitmq/sbin/rabbitmq-plugins enable rabbitmq_management

# Create vhost and user for Forge
sudo -u rabbitmq /opt/rabbitmq/sbin/rabbitmqctl add_vhost master
sudo -u rabbitmq /opt/rabbitmq/sbin/rabbitmqctl add_user forged changeme-use-a-strong-password
sudo -u rabbitmq /opt/rabbitmq/sbin/rabbitmqctl set_permissions -p master forged ".*" ".*" ".*"
```

### Verify

```bash
sudo -u rabbitmq /opt/rabbitmq/sbin/rabbitmqctl status
```

## 4. Install Forge

### Create User and Directories

```bash
sudo useradd -d /opt/forge -s /usr/bin/false forged
sudo mkdir -p /opt/forge/bin /opt/forge/lib/svc/method
sudo mkdir -p /etc/forged
sudo mkdir -p /var/lib/forged/jj-repos /var/lib/forged/acme
sudo chown -R forged:forged /var/lib/forged
```

### Install the Binaries

If you have pre-built illumos binaries:

```bash
sudo cp forged /opt/forge/bin/
sudo cp pkgdev /opt/forge/bin/
sudo chmod +x /opt/forge/bin/*
```

If building from source on the machine:

```bash
# Install build dependencies
sudo pkg install developer/gcc-13 developer/build/gnu-make \
  library/security/openssl-31 system/header \
  developer/build/pkg-config library/libarchive

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Build
cargo build -p forged -p pkgdev --release
sudo cp target/release/forged target/release/pkgdev /opt/forge/bin/
```

### Install the SMF Manifest and Method Script

```bash
sudo cp smf/forged.xml /lib/svc/manifest/application/forge-forged.xml
sudo cp smf/forged-method /opt/forge/lib/svc/method/forged-method
sudo chmod +x /opt/forge/lib/svc/method/forged-method
```

### Write the Configuration File

Create `/etc/forged/forged.toml`:

```toml
[server]
listen_addr = "0.0.0.0:50051"

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
sudo chown root:forged /etc/forged/forged.toml
sudo chmod 640 /etc/forged/forged.toml
```

### Import and Enable the Service

```bash
sudo svccfg import /lib/svc/manifest/application/forge-forged.xml
sudo svcadm enable forge/forged
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

### Allow Binding to Privileged Ports

On illumos, non-root processes cannot bind ports below 1024 by default. Grant the privilege:

```bash
sudo usermod -K defaultpriv=basic,net_privaddr forged
```

Alternatively, use port forwarding with `ipnat` or `ipfilter`:

```bash
# Forward port 80 -> 8080 and port 443 -> 50051 (if needed)
echo "rdr e1000g0 0/0 port 80 -> 127.0.0.1 port 8080 tcp" | sudo ipnat -f -
```

If using port forwarding, update the config accordingly:

```toml
[tls.acme]
http_listen_addr = "0.0.0.0:8080"
```

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
# Allow gRPC (TLS)
pass in on e1000g0 proto tcp from any to any port = 50051

# Allow HTTP for ACME challenges
pass in on e1000g0 proto tcp from any to any port = 80

# Allow SSH (for administration)
pass in on e1000g0 proto tcp from any to any port = 22

# Block everything else from outside
block in on e1000g0 all
```

Apply:

```bash
sudo svcadm enable ipfilter
sudo ipf -Fa -f /etc/ipf/ipf.conf
```

Internal services (PostgreSQL 5432, SeaweedFS 9333/8080, RabbitMQ 5672) should only be accessible from localhost. The default configuration binds them to `127.0.0.1`, which is already secure.

## 7. Verify the Full Stack

Check all services are running:

```bash
svcs -a | grep -E 'postgres|seaweedfs|rabbitmq|forge'
```

Expected output:

```
online  svc:/application/database/postgresql:version_17
online  svc:/application/seaweedfs/master:default
online  svc:/application/seaweedfs/volume:default
online  svc:/application/rabbitmq:default
online  svc:/application/forge/forged:default
```

Test the gRPC endpoint:

```bash
# Without TLS
grpcurl -plaintext localhost:50051 grpc.health.v1.Health/Check

# With TLS
grpcurl forge.example.com:50051 grpc.health.v1.Health/Check
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
sudo cp scripts/backup.sh /opt/forge/bin/
sudo chmod +x /opt/forge/bin/backup.sh

# Run a backup
sudo -u forged FORGED_POSTGRES_URL="postgresql://forged:password@localhost/forged" \
  /opt/forge/bin/backup.sh /var/backups/forged
```

Schedule daily backups with cron:

```bash
sudo crontab -e -u forged
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
pkgdev auth register \
  --host https://forge.example.com:50051 \
  --actor-id admin \
  --email admin@example.com \
  --public-key ~/.ssh/id_ed25519.pub
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
- **"Failed to bind"** -- Check if the port is already in use (`netstat -an | grep 50051`)
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
sudo -u rabbitmq /opt/rabbitmq/sbin/rabbitmqctl status
# Check vhost exists:
sudo -u rabbitmq /opt/rabbitmq/sbin/rabbitmqctl list_vhosts
# Check user permissions:
sudo -u rabbitmq /opt/rabbitmq/sbin/rabbitmqctl list_permissions -p master
```

### ACME Certificate Issues

```bash
# Check if port 80 is reachable from outside
curl http://forge.example.com/.well-known/acme-challenge/test

# Check cached certificates
ls -la /var/lib/forged/acme/

# Force certificate renewal by removing cache
sudo rm /var/lib/forged/acme/cert.pem /var/lib/forged/acme/key.pem
sudo svcadm restart forge/forged
```

## 10. Upgrading

```bash
# Stop the service
sudo svcadm disable forge/forged

# Replace binaries
sudo cp new-forged /opt/forge/bin/forged
sudo cp new-pkgdev /opt/forge/bin/pkgdev

# Start the service (migrations run automatically)
sudo svcadm enable forge/forged

# Verify
svcs forge/forged
```
