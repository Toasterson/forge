# Components

A **component** represents a piece of software that can be built and installed onto a system. Each component lives within a [gate](./gates.md) and is defined by a `package.kdl` recipe file.

## What a Component Contains

A component directory typically includes:

- **`package.kdl`** -- the declarative recipe defining how to fetch, build, and package the software
- **Patches** -- files that modify upstream source code for compatibility or policy
- **Scripts** -- custom build or installation scripts
- **License files** -- license text bundled with the package
- **Overlay files** -- additional files to include in the package

## Source-to-Package Mapping

Components are flexible in how they map sources to packages:

- **One-to-one** -- A single source produces a single package (most common)
- **One-to-many** -- A single source is split into multiple packages (e.g., a library component producing `library/foo` and `developer/foo` packages)
- **Many-to-one** -- Multiple sources are combined into a single package

Package splitting is controlled by the `package` sections in `package.kdl`. See [Package Splitting](../packaging-guide/package-splitting.md) for details.

## Component Lifecycle

1. **Create** -- Write a `package.kdl` recipe and place it in the gate's components directory
2. **Upload** -- Push the component to the Forge server using `pkgdev`
3. **Build** -- Submit a build request; Forge dispatches it to a CI worker
4. **Publish** -- Built packages are stored and can be published to a repository

## Example

A minimal component for the `zlib` compression library:

```
components/library/zlib/
  package.kdl
```

```kdl
name "library/zlib"
summary "The zlib Compression Library"
version "1.3.1"

source {
    archive "https://zlib.net/zlib-1.3.1.tar.xz" \
        sha256="38ef96b8dfe510d42707d9c781877914792541133e1870841463bfa73f883e32"
}

build {
    configure {
        option "--prefix=/usr"
    }
}
```
