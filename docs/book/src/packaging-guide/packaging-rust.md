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

## Tips

- The `cargo` keyword takes no arguments -- Cargo configuration comes from `Cargo.toml` in the source
- Ensure `system/library` and `system/library/gcc-runtime` are listed as runtime dependencies
- For projects with C dependencies, those libraries must be available on the build host and declared as build dependencies with `dev=true`
- Use an `archive` source with a tag-based URL for reproducible builds
