# Introduction

Forge is an experimental, service-oriented code forge and packaging platform built in Rust. It targets illumos/Solaris packaging workflows and provides a complete pipeline from source code to installable IPS packages.

## What is Forge?

Forge brings together version control, package metadata, build orchestration, and artifact distribution into a single platform. It is designed around the needs of illumos distributions that use the Image Packaging System (IPS) but is flexible enough to produce other distribution formats such as tarballs.

Key capabilities:

- **Declarative package definitions** in KDL format (`package.kdl`) describing sources, build steps, dependencies, and packaging rules
- **Gate management** for organizing components under shared policy, versioning, and publisher identity
- **Source management** with support for archives, git repositories, patches, and file overlays
- **Build orchestration** dispatching builds to Solstice CI workers via AMQP
- **Artifact storage** using SeaweedFS for content-addressable blob storage
- **gRPC API** for programmatic access to all platform operations
- **`pkgdev` CLI** for package developers to interact with the platform

## Status

Forge is in **pre-alpha**. APIs, data formats, and behavior may change without notice. It is not yet suitable for production use.

## License

Forge is licensed under the [Mozilla Public License 2.0 (MPL-2.0)](https://www.mozilla.org/en-US/MPL/2.0/).
