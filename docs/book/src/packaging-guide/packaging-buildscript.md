# Packaging with Build Scripts

For software that uses non-standard build systems or requires custom build steps, use the `script` build type. This gives you full control over the build and install process.

## Basic Structure

```kdl
name "utility/example"
summary "An example package with a custom build"
version "1.0.0"

source {
    archive "https://example.com/example-1.0.0.tar.gz" \
        sha256="abc123..."
}

build {
    script {
        script "build.sh" prototype-dir="proto"
        install src="build/bin" target="usr/bin" name="example"
    }
}
```

## The Build Script

The build script is a shell script placed in the component directory. It is executed with the source directory as the working directory.

```bash
#!/bin/bash
set -e

# Build
make CC=gcc CFLAGS="-m64 -O2"

# Install to the prototype directory
make install PREFIX=/usr DESTDIR=$PROTO_DIR
```

The `prototype-dir` property on the `script` node specifies where build output is staged before packaging.

## Install Rules

The `install` node provides declarative file installation without a build script:

```kdl
build {
    script {
        script "build.sh" prototype-dir="proto"
        install src="build/bin" target="usr/bin" name="myapp"
        install src="build/lib" target="usr/lib" pattern="*.so*" match="glob"
        install src="conf" target="etc/myapp" pattern="*.conf"
    }
}
```

| Property | Description |
|---|---|
| `src` | Source path within the build directory |
| `target` | Destination path in the prototype directory |
| `name` | Specific filename to install |
| `pattern` | Glob or regex pattern for matching files |
| `match` | Pattern type: `"glob"` or `"regex"` |

## Example: A Go Project

```kdl
name "utility/hugo"
summary "A fast static site generator"
version "0.124.0"
project-url "https://gohugo.io/"

source {
    archive "https://github.com/gohugoio/hugo/archive/v0.124.0.tar.gz" \
        sha256="abc123..."
}

build {
    script {
        script "build-go.sh" prototype-dir="proto"
        install src="." target="usr/bin" name="hugo"
    }
}

dependency "system/library" kind="require"
```

With `build-go.sh`:

```bash
#!/bin/bash
set -e
export GOPATH=$(pwd)/.go
go build -o hugo .
mkdir -p $PROTO_DIR/usr/bin
cp hugo $PROTO_DIR/usr/bin/
```

## Overlay Files

For packages that deliver configuration files or non-compiled assets, combine `overlay` sources with a minimal script:

```kdl
name "system/config/example"
summary "Example configuration files"
version "1.0"

source {
    overlay "files"
}

build {
    script {
        script "install.sh" prototype-dir="proto"
    }
}
```

Where `files/` contains the directory tree to install and `install.sh` copies it into the prototype directory.

## Tips

- Always use `set -e` in build scripts so failures are caught immediately
- The prototype directory (`PROTO_DIR`) is the root of the package filesystem tree
- Install files under their final system paths within the prototype (e.g., `$PROTO_DIR/usr/bin/myapp`)
- Use `install` nodes for simple file copies; reserve the script for complex build logic
