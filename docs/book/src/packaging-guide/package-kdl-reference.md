# package.kdl Reference

This is the complete reference for the `package.kdl` component recipe format. Every section and field is documented here with examples.

## Top-level Fields

| Field | Required | Repeatable | Description |
|---|---|---|---|
| `name` | yes | no | Package FMRI name (e.g., `"web/curl"`, `"library/zlib"`) |
| `project-name` | no | no | Upstream project name if different from the package name |
| `classification` | no | no | IPS classification (e.g., `"System/Libraries"`) |
| `summary` | no | no | One-line human-readable description |
| `license` | no | no | License identifier (e.g., `"MIT"`, `"GPLv2 with Classpath Exception"`) |
| `license-file` | no | no | Path to license file relative to the component directory |
| `prefix` | no | no | Installation prefix (e.g., `"/usr"`) |
| `version` | no | no | Software version |
| `revision` | no | no | Package revision number |
| `project-url` | no | no | Upstream project URL |
| `maintainer` | no | yes | Package maintainer name or identifier |
| `seperate-build-dir` | no | no | Boolean flag; use a separate build directory |

### Example

```kdl
name "web/curl"
project-name "curl"
classification "System/Libraries"
summary "The CURL Network Utility and Library"
license "CURL"
license-file "COPYING"
prefix "/usr"
version "8.6.0"
revision "1"
project-url "https://curl.se"
maintainer "The OpenIndiana Maintainers"
```

## Metadata

The `metadata` block holds arbitrary key-value pairs. These are used for integration with external tracking systems and are not interpreted by the build system.

```kdl
metadata {
    anitya-id "381"
    repology-id "curl"
    upstream-version "8.6.0"
    custom-key "custom-value"
}
```

Each child node has a string name (the key) and a string argument (the value).

## Source

The `source` block defines where to obtain source code and supplemental files. It contains one or more source nodes of different types.

### archive

Downloads a source archive (tarball, zip, etc.) from a URL.

```kdl
source {
    archive "https://curl.haxx.se/download/curl-8.6.0.tar.xz" \
        sha256="3ccd55d91af9516539df80625f818c734dc6f2ecf9bada33c76765e99121db15"
}
```

| Property | Required | Description |
|---|---|---|
| *(argument)* | yes | Download URL |
| `sha256` | no | SHA-256 checksum of the archive |
| `sha512` | no | SHA-512 checksum of the archive |
| `signature-url` | no | URL of a detached signature file |
| `signature-url-extension` | no | File extension to append to the archive URL to form the signature URL |

### git

Clones a git repository.

```kdl
source {
    git "https://github.com/example/project.git" \
        tag="v1.2.3" \
        archive=false \
        must-stay-as-repo=false \
        directory="project-src"
}
```

| Property | Required | Description |
|---|---|---|
| *(argument)* | yes | Repository URL |
| `branch` | no | Branch to clone |
| `tag` | no | Tag to check out |
| `archive` | no | Convert the clone to a tarball (`true`/`false`) |
| `must-stay-as-repo` | no | Keep as a git repo during build (`true`/`false`) |
| `directory` | no | Target directory name (useful with multiple git sources) |

### patch

A patch file to apply to the extracted source.

```kdl
source {
    patch "fix-makefile.patch" drop-directories=1
}
```

| Property | Required | Description |
|---|---|---|
| *(argument)* | yes | Path to the patch file relative to the component directory |
| `drop-directories` | no | Number of leading directory components to strip (like `patch -p`) |

### file

Copy a single file into the source tree.

```kdl
source {
    file "files/config.toml" "etc/myapp/config.toml"
}
```

| Argument | Position | Description |
|---|---|---|
| bundle path | 1st | Path to the file in the component directory |
| target path | 2nd | Destination path in the source tree |

### directory

Copy an entire directory into the source tree.

```kdl
source {
    directory "files/conf.d" "etc/myapp/conf.d"
}
```

| Argument | Position | Description |
|---|---|---|
| bundle path | 1st | Path to the directory in the component directory |
| target path | 2nd | Destination path in the source tree |

### overlay

Overlay a directory on top of the extracted source. Files in the overlay replace files in the source tree at matching paths.

```kdl
source {
    overlay "overlay"
}
```

| Argument | Position | Description |
|---|---|---|
| bundle path | 1st | Path to the overlay directory in the component directory |

### Combining Sources

A component can use multiple source nodes. They are processed in order:

```kdl
source {
    archive "https://example.com/project-1.0.tar.gz" \
        sha256="abc123..."
    patch "001-fix-build.patch" drop-directories=1
    patch "002-add-feature.patch" drop-directories=1
    overlay "files"
    file "Makefile.illumos" "Makefile"
}
```

## Build

The `build` block describes how to compile the software. Each build block optionally takes a source directory argument specifying which extracted source to build from (useful when a component has multiple sources).

```kdl
build "project-src" {
    configure { ... }
}
```

Only one build system keyword should be used per `build` block. Multiple `build` blocks are allowed for components that require separate build passes.

### configure

For autoconf / `./configure`-based projects. This is the most common build system for C and C++ software.

```kdl
build {
    configure {
        compiler "gcc"
        linker "ld"
        enable-large-files
        disable-destdir-option

        option "--prefix=/usr"
        option "--sysconfdir=/etc"
        option "--enable-shared"
        option "--disable-static"
        option "--with-ssl=/usr/openssl/3.1"

        flag "-m64" name="CFLAGS"
        flag "-m64" name="LDFLAGS"
        flag "-I/usr/include/security" name="CPPFLAGS"
    }
}
```

#### configure children

| Node | Repeatable | Description |
|---|---|---|
| `option` | yes | A `./configure` option (passed as-is) |
| `flag` | yes | A compiler/linker flag; `name` specifies the variable (e.g., `CFLAGS`, `LDFLAGS`) |
| `compiler` | no | Override the C compiler (e.g., `"gcc"`, `"/opt/gcc-13/bin/gcc"`) |
| `linker` | no | Override the linker (e.g., `"ld"`, `"/usr/bin/ld"`) |
| `enable-large-files` | no | Boolean flag; add large file support options |
| `disable-destdir-option` | no | Boolean flag; skip DESTDIR-related configure options |

### cmake

For CMake-based projects.

```kdl
build {
    cmake "-DCMAKE_INSTALL_PREFIX=/usr" "-DBUILD_SHARED_LIBS=ON"
}
```

The arguments to `cmake` are passed directly to the cmake invocation.

### meson

For Meson-based projects.

```kdl
build {
    meson "--prefix=/usr" "--buildtype=release"
}
```

The arguments to `meson` are passed directly to the meson setup invocation.

### cargo

For Rust projects using Cargo.

```kdl
build {
    cargo
}
```

No additional configuration is needed. Forge invokes `cargo build --release` and installs the resulting binaries.

### script

For projects with custom or non-standard build systems.

```kdl
build {
    script {
        script "build.sh" prototype-dir="proto"
        install src="build/bin" target="usr/bin" name="myapp"
        install src="build/lib" target="usr/lib" pattern="*.so*" match="glob"
    }
}
```

#### script children

| Node | Description |
|---|---|
| `script` | Script filename to execute; `prototype-dir` sets the staging directory |
| `install` | Install rule with `src`, `target`, `name`, `pattern`, and `match` properties |

## Dependency

Dependencies declare what other packages are required to build or run this component. Each dependency is a separate top-level node.

```kdl
dependency "library/zlib" dev=true kind="require"
dependency "library/openssl-31" dev=true kind="require"
dependency "system/library" kind="require"
dependency "runtime/python-312" kind="optional"
dependency "consolidation/userland/userland-incorporation" kind="incorporate"
```

| Property | Required | Default | Description |
|---|---|---|---|
| *(argument)* | yes | -- | Package FMRI of the dependency |
| `dev` | no | `false` | Whether this is a build-time (developer) dependency |
| `kind` | no | `"require"` | Dependency type: `"require"`, `"incorporate"`, or `"optional"` |

### Dependency Kinds

- **`require`** -- The package must be installed. This is the default.
- **`incorporate`** -- Version-locks the dependency to the gate's version. Used for consolidation incorporations.
- **`optional`** -- The package may be installed; if present, it must satisfy version constraints.

## Package

The `package` section controls how build output is split into installable packages. Without any `package` sections, all output goes into a single package named after the component.

Each `package` block optionally takes a name argument for the sub-package FMRI suffix. The default (unnamed) package block defines the main package.

```kdl
package {
    file path="usr/lib/.*\\.so\\..*"
    file path="usr/bin/.*"
    link path="usr/lib/.*\\.so$"
}

package "developer/library/zlib" {
    file path="usr/include/.*"
    file path="usr/lib/.*\\.a$"
    file path="usr/lib/pkgconfig/.*\\.pc$"
    file path="usr/share/man/man3/.*"
}
```

### Package children

Each child node is a manifest transform selector. The `path` property is a regular expression matched against file paths in the build output.

| Node | Description |
|---|---|
| `file` | Match regular files |
| `link` | Match symbolic links |
| `hardlinks` | Match hard links |

Each node accepts arbitrary properties that serve as selectors or attribute overrides in the IPS manifest:

| Property | Description |
|---|---|
| `path` | Regex pattern matched against the installed file path |
| `mode` | Override the file mode |
| `owner` | Override the file owner |
| `group` | Override the file group |
| `action` | IPS action type |

## Complete Example

Here is a complete `package.kdl` exercising all sections:

```kdl
name "web/curl"
project-name "curl"

metadata {
    anitya-id "381"
    repology-id "curl"
}

classification "System/Libraries"
summary "The CURL Network Utility and Library"
license "CURL"
license-file "COPYING"
version "8.6.0"
revision "1"
project-url "https://curl.se"
maintainer "The OpenIndiana Maintainers"

source {
    archive "https://curl.haxx.se/download/curl-8.6.0.tar.xz" \
        sha256="3ccd55d91af9516539df80625f818c734dc6f2ecf9bada33c76765e99121db15"
    patch "000-configure.ac.patch"
    patch "005-libcurl.pc.in.patch"
}

build {
    configure {
        option "--prefix=/usr"
        option "--mandir=/usr/share/man"
        option "--bindir=/usr/bin"
        option "--libdir=/usr/lib"
        option "--enable-shared"
        option "--disable-static"
        option "--enable-ipv6"
        option "--with-ssl=/usr/openssl/3.1"
        option "--with-zlib=/usr"
        option "--with-ca-bundle=/etc/certs/ca-certificates.crt"

        flag "-m64" name="CFLAGS"
        flag "-m64" name="LDFLAGS"
        flag "-I/usr/openssl/3.1/include" name="CPPFLAGS"
        flag "-L/usr/openssl/3.1/lib" name="LDFLAGS"
    }
}

dependency "library/zlib" dev=true kind="require"
dependency "library/openssl-31" dev=true kind="require"
dependency "library/libssh2" dev=true kind="require"
dependency "library/nghttp2" dev=true kind="require"
dependency "system/library" kind="require"

package {
    file path="usr/bin/.*"
    file path="usr/lib/.*\\.so\\..*"
    link path="usr/lib/.*\\.so$"
    file path="usr/share/man/man1/.*"
}

package "developer/web/curl" {
    file path="usr/include/.*"
    file path="usr/lib/.*\\.a$"
    file path="usr/lib/pkgconfig/.*"
    file path="usr/share/man/man3/.*"
}
```
