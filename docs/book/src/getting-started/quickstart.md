# Quick Start

This guide walks through setting up a local Forge instance and creating your first component package.

## 1. Start the Services

Forge requires PostgreSQL and SeaweedFS. Start them with Docker Compose:

```bash
docker compose -f docker-compose.dev.yml up -d
```

Then start the Forge server:

```bash
cargo run -p forged
```

## 2. Authenticate

Log in to the Forge instance using your OIDC identity:

```bash
pkgdev auth login --host http://localhost:50051
```

Follow the instructions to visit the OIDC provider URL and enter the code. After authorization, optionally add your SSH key:

```bash
pkgdev auth add-key --public-key ~/.ssh/id_ed25519.pub --key-id default
```

## 3. Create a Gate

Write a `gate.kdl` file defining your packaging gate:

```kdl
name "my-packages"
version "0.5.11"
branch "2024.0.0"
publisher "example.com"
```

Open the gate on the server:

```bash
pkgdev forge gate open \
  --host http://localhost:50051 \
  --name my-packages \
  --owner-id myuser \
  --owner-kind user
```

Upload the gate definition:

```bash
pkgdev forge gate upload --host http://localhost:50051 --owner-id myuser
```

## 4. Create a Component

Create a directory for your component and write a `package.kdl` file:

```bash
mkdir -p components/library/zlib
```

```kdl
name "library/zlib"
project-name "zlib"
classification "System/Libraries"
summary "The zlib Compression Library"
license "zlib License"
version "1.3.1"
project-url "https://zlib.net"

source {
    archive "https://zlib.net/zlib-1.3.1.tar.xz" \
        sha256="38ef96b8dfe510d42707d9c781877914792541133e1870841463bfa73f883e32"
}

build {
    configure {
        option "--prefix=/usr"
        option "--shared"
    }
}

dependency "system/library" kind="require"
```

Upload the component:

```bash
pkgdev forge component upload --host http://localhost:50051
```

## 5. Build

Submit a build for your component:

```bash
pkgdev build --component components/library/zlib
```

The build is dispatched to a Solstice CI worker. Check the build status:

```bash
pkgdev forge component show --host http://localhost:50051 <COMPONENT_ID>
```
