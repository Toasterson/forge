# Packaging Rust Software

Rust projects using Cargo are handled with the `cargo` keyword. This is the simplest build configuration in Forge.

## Basic Structure

```kdl
name "utility/ripgrep"
summary "A fast line-oriented search tool"
version "14.1.0"
project-url "https://github.com/BurntSushi/ripgrep"

source {
    archive "https://github.com/BurntSushi/ripgrep/archive/14.1.0.tar.gz" \
        sha256="abc123..."
}

build {
    cargo
}
```

Forge runs `cargo build --release` and installs the resulting binaries. No additional configuration is needed for most Rust projects.

## Cargo Options

For projects that need more control over the build, the `cargo` keyword accepts a block:

```kdl
build {
    cargo {
        packages "forged" "pkgdev"
        features "otel" "quic"
        install-root "/opt/forge"
        target "x86_64-unknown-illumos"
        env OPENSSL_DIR="/usr/openssl/3.1"
        env PKG_CONFIG_PATH="/usr/lib/amd64/pkgconfig"
    }
}
```

### packages

Select which crates to build from a workspace. Without this, all workspace binaries are built.

```kdl
cargo {
    packages "forged" "pkgdev"
}
```

### features

Enable specific Cargo features:

```kdl
cargo {
    features "otel" "quic"
}
```

### install-root

Change the installation prefix. The default is `/usr`, which installs binaries to `/usr/bin`. Setting it to `/opt/forge` installs to `/opt/forge/bin`.

```kdl
cargo {
    install-root "/opt/forge"
}
```

### target

Specify a Rust target triple for cross-compilation:

```kdl
cargo {
    target "x86_64-unknown-illumos"
}
```

### env

Set environment variables for the build. Each `env` node sets one or more variables as key-value properties. Repeatable.

```kdl
cargo {
    env OPENSSL_DIR="/usr/openssl/3.1"
    env PKG_CONFIG_PATH="/usr/lib/amd64/pkgconfig"
}
```

### offline and locked

By default, Forge passes `--offline` and `--locked` to cargo for reproducible builds. These are both enabled by default. If you need to disable them, omit them from the cargo block (they default to `true` when the cargo block is present).

## Git Source

For Rust projects that are not published as release tarballs:

```kdl
name "utility/my-tool"
summary "A custom Rust utility"
version "0.1.0"

source {
    git "https://github.com/example/my-tool.git" tag="v0.1.0"
}

build {
    cargo
}
```

## Example: A CLI Tool

```kdl
name "developer/tool/just"
project-name "just"
summary "A command runner for project-specific tasks"
version "1.25.0"
license "CC0-1.0"
project-url "https://github.com/casey/just"
maintainer "The OpenIndiana Maintainers"

metadata {
    repology-id "just"
}

source {
    archive "https://github.com/casey/just/archive/1.25.0.tar.gz" \
        sha256="def456..."
}

build {
    cargo
}

dependency "system/library" kind="require"
dependency "system/library/gcc-runtime" kind="require"
```

## Example: A Workspace with Multiple Binaries

This example builds two binaries from a Rust workspace and installs them under `/opt/forge`:

```kdl
name "developer/packaging/forge"
project-name "forge"
summary "Code forge and IPS packaging platform"
license "MPL-2.0"
version "0.1.0"

source {
    git "https://github.com/Toasterson/forge.git" tag="v0.1.0"
}

build {
    cargo {
        packages "forged" "pkgdev"
        install-root "/opt/forge"
        env OPENSSL_DIR="/usr/openssl/3.1"
        env PKG_CONFIG_PATH="/usr/lib/amd64/pkgconfig"
    }
}

dependency "system/library" kind="require"
dependency "system/library/gcc-14-runtime" kind="require"
dependency "library/security/openssl-31" kind="require"

package {
    file path="opt/forge/bin/.*"
}
```

## Tips

- For simple projects, `cargo` with no block works perfectly -- Cargo configuration comes from `Cargo.toml` in the source
- Use `packages` to select specific binaries from a workspace instead of building everything
- Use `install-root` when the software should live outside the standard `/usr` prefix
- Use `env` to point to non-standard library locations (OpenSSL, pkg-config paths, etc.)
- Ensure `system/library` and `system/library/gcc-runtime` (or the specific GCC version) are listed as runtime dependencies
- For projects with C dependencies, those libraries must be available on the build host and declared as build dependencies with `dev=true`
- Use an `archive` source with a tag-based URL for reproducible builds
