# Distributions

A **distribution** defines the output format for packages produced by a gate. Forge supports multiple distribution types to accommodate different deployment models.

## Distribution Types

### IPS (Image Packaging System)

The default distribution type. Packages are produced in the IPS format used by illumos and Solaris distributions. IPS provides:

- Dependency resolution and installation
- Self-assembly with variants and facets
- Content-addressable package storage
- Publisher-based repository model

```kdl
distribution {
    type "ips"
}
```

### Tarball

Packages are produced as compressed tar archives. This is useful for environments that don't use IPS or for distributing software as standalone bundles.

```kdl
distribution {
    type "tarball"
}
```

## Configuring Distribution

The distribution type is set in the gate's `gate.kdl` file. If omitted, the default is `ips`.

```kdl
name "my-gate"
version "0.5.11"
branch "2024.0.0"
publisher "example.com"

distribution {
    type "ips"
}
```

All components within a gate share the same distribution type. The distribution determines how build output is processed and what final artifacts are produced.
