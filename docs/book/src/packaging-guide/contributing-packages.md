# Contributing Packages

This guide covers how to contribute new component packages to a Forge gate.

## Prerequisites

- A working Forge instance (see [Quick Start](../getting-started/quickstart.md))
- `pkgdev` CLI installed and authenticated
- Familiarity with the [package.kdl format](./package-kdl-reference.md)

## Workflow

### 1. Create the Component Directory

Components live under the gate's `components/` directory, organized by category:

```bash
mkdir -p components/library/mylib
```

### 2. Write the Recipe

Create `components/library/mylib/package.kdl`:

```kdl
name "library/mylib"
project-name "mylib"
classification "System/Libraries"
summary "My Example Library"
license "MIT"
license-file "LICENSE"
version "1.0.0"
project-url "https://example.com/mylib"
maintainer "Your Name"

metadata {
    repology-id "mylib"
}

source {
    archive "https://example.com/mylib-1.0.0.tar.gz" \
        sha256="..."
}

build {
    configure {
        option "--prefix=/usr"
        option "--enable-shared"
        option "--disable-static"
    }
}

dependency "system/library" kind="require"
```

### 3. Add Patches (if needed)

Place patch files alongside `package.kdl` and reference them in the `source` block:

```
components/library/mylib/
  package.kdl
  001-fix-headers.patch
  002-illumos-compat.patch
```

```kdl
source {
    archive "https://example.com/mylib-1.0.0.tar.gz" sha256="..."
    patch "001-fix-headers.patch" drop-directories=1
    patch "002-illumos-compat.patch" drop-directories=1
}
```

### 4. Test Locally

Download and verify sources:

```bash
pkgdev download --component components/library/mylib
```

Generate and review metadata:

```bash
pkgdev metadata --component components/library/mylib
```

### 5. Upload

Upload the component to the Forge server:

```bash
pkgdev forge component upload --host http://forge.example.com:50051
```

### 6. Build and Verify

Submit a build:

```bash
pkgdev build --component components/library/mylib
```

## Naming Conventions

Component names follow IPS FMRI conventions:

| Category | Pattern | Example |
|---|---|---|
| Libraries | `library/<name>` | `library/zlib` |
| Developer tools | `developer/<name>` | `developer/cmake` |
| System utilities | `system/<name>` | `system/gnu-tar` |
| Web software | `web/<name>` | `web/curl` |
| Runtimes | `runtime/<lang>/<name>` | `runtime/python-312` |
| Services | `service/<name>` | `service/network/dns/bind` |

## Checklist

Before submitting a component, verify:

- [ ] `name` follows FMRI naming conventions
- [ ] `version` matches the upstream release being packaged
- [ ] Source archive has a `sha256` checksum
- [ ] All patches apply cleanly and have descriptive filenames
- [ ] Build options are appropriate for illumos
- [ ] Runtime dependencies are listed (not just build-time)
- [ ] License file is included and `license` field is set
- [ ] Package splitting separates runtime and developer files (for libraries)
