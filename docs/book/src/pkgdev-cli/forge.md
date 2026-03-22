# Forge Commands

The `pkgdev forge` commands interact with a remote Forge server to manage gates and components.

## Gate Operations

### Open a Gate

Create a new gate on the server:

```bash
pkgdev forge gate open \
  --host http://localhost:50051 \
  --id <GATE_UUID> \
  --name my-packages \
  --owner-id myuser \
  --owner-kind user
```

### Upload a Gate Definition

Upload the local `gate.kdl` to the server:

```bash
pkgdev forge gate upload \
  --host http://localhost:50051 \
  --owner-id myuser
```

### List Gates

```bash
pkgdev forge gate list --host http://localhost:50051
pkgdev forge gate list --host http://localhost:50051 --no-header
```

### Show Gate Details

```bash
pkgdev forge gate show \
  --host http://localhost:50051 \
  --id <GATE_UUID>
```

## Component Operations

### Create a Component

```bash
pkgdev forge component create \
  --host http://localhost:50051 \
  --id <GATE_UUID> \
  --name library/zlib
```

### Upload a Component

Upload the local component (recipe, patches, files) to the server:

```bash
pkgdev forge component upload \
  --host http://localhost:50051 \
  [COMPONENT_PATH]
```

If `COMPONENT_PATH` is omitted, the current directory is used.

### List Components

```bash
pkgdev forge component list --host http://localhost:50051
pkgdev forge component list --host http://localhost:50051 --no-header
```

### Show Component Details

```bash
pkgdev forge component show \
  --host http://localhost:50051 \
  <COMPONENT_ID>
```
