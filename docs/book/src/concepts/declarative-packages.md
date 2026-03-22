# Declarative Packages

Forge uses a **declarative** approach to package definitions. Rather than writing imperative build scripts, you describe *what* a package is -- its sources, build system, dependencies, and packaging rules -- in a structured `package.kdl` file. Forge interprets this description to fetch sources, drive the build, and produce installable packages.

## Why Declarative?

- **Repeatable builds** -- The same recipe produces the same output regardless of who runs it or where
- **Single source of truth** -- All package metadata, build instructions, and dependencies live in one file
- **Tooling-friendly** -- Structured data is easy to validate, transform, and query programmatically
- **Portability** -- Recipes are independent of the build host's environment

## The package.kdl File

Every component has a `package.kdl` file written in [KDL](https://kdl.dev), a document language designed for configuration. The file contains these sections:

### Identity

```kdl
name "library/zlib"
project-name "zlib"
classification "System/Libraries"
summary "The zlib Compression Library"
license "zlib License"
license-file "LICENSE"
version "1.3.1"
revision "1"
project-url "https://zlib.net"
maintainer "The OpenIndiana Maintainers"
```

### Metadata

Arbitrary key-value pairs for tracking and integration with external systems:

```kdl
metadata {
    anitya-id "5303"
    repology-id "zlib"
}
```

### Sources

Where to get the upstream code, patches, and additional files:

```kdl
source {
    archive "https://zlib.net/zlib-1.3.1.tar.xz" \
        sha256="38ef96b8dfe510d42707d9c781877914792541133e1870841463bfa73f883e32"
    patch "fix-configure.patch" drop-directories=1
    overlay "files"
}
```

### Build

How to compile the software:

```kdl
build {
    configure {
        option "--prefix=/usr"
        option "--shared"
    }
}
```

### Dependencies

What other packages are required at build time or runtime:

```kdl
dependency "system/library" kind="require"
dependency "developer/build/make" dev=true kind="require"
```

### Packages

How to split build output into installable packages:

```kdl
package {
    file path="usr/lib/.*\\.so\\..*"
}
package "developer/zlib" {
    file path="usr/include/.*"
    file path="usr/lib/.*\\.a$"
    file path=".*\\.pc$"
}
```

For the complete syntax, see the [package.kdl Reference](../packaging-guide/package-kdl-reference.md).

## Supported Build Systems

Forge understands several build systems natively:

| Build System | Keyword | Typical Use |
|---|---|---|
| Autoconf / configure | `configure` | Most C/C++ projects |
| CMake | `cmake` | Cross-platform C/C++ projects |
| Meson | `meson` | Modern C/C++ projects |
| Cargo | `cargo` | Rust projects |
| Custom script | `script` | Anything else |

## Use Cases

- **Tarball + autotools** -- The most common case. Download a release tarball, apply patches, run `./configure && make && make install`.
- **Git-based builds** -- Clone a repository at a specific tag or branch and build from source.
- **Non-building assets** -- Package configuration files, documentation, or other assets that don't require compilation.
- **Split outputs** -- Build a library once and split it into runtime and developer packages.
