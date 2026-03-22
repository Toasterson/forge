# Building

## Local Operations

### Download Sources

Download and verify source archives for a component:

```bash
pkgdev download --component path/to/component
```

This fetches all archives listed in the `source` block, verifies checksums, and extracts them.

### Generate Metadata

Extract metadata from a component recipe in various formats:

```bash
pkgdev metadata --component path/to/component
pkgdev metadata --component path/to/component json
pkgdev metadata --component path/to/component toml
pkgdev metadata --component path/to/component yaml
```

### Generate Schemas and Manifests

```bash
# Generate a JSON schema for the component recipe format
pkgdev generate component-recipe

# Generate a forge integration manifest
pkgdev generate forge-integration-manifest

# Generate a repology report
pkgdev generate repology

# Write output to a file
pkgdev generate component-recipe --output schema.json
```

### Create a New Component

Scaffold a new component from an FMRI:

```bash
pkgdev create library/zlib --component components/library/zlib
```

### Edit a Component

Edit component properties locally:

```bash
pkgdev edit --component path/to/component <SUBCOMMAND>
```

## Remote Builds

### Submit a Build

```bash
pkgdev build --component path/to/component
```

This uploads the component to the Forge server and dispatches a build to a Solstice CI worker via AMQP. The build runs on an illumos host and produces IPS packages or tarballs depending on the gate's distribution type.
