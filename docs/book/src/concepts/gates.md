# Gates

A **gate** is the primary organizational unit in Forge. It groups components under a shared set of policies, versioning, and publisher identity. The concept originates from the consolidation model used at Sun Microsystems for managing OpenSolaris.

## What a Gate Contains

A gate provides:

- **Components** -- the individual software packages managed within the gate
- **Central policy** -- shared build settings, compiler flags, and packaging conventions
- **Transforms** -- rules that modify IPS manifests across all components (e.g., setting default file ownership, applying facets)
- **Branch policy** -- the IPS version and branch string applied to all packages
- **Publisher identity** -- the IPS publisher name for all packages produced by the gate
- **Metadata transforms** -- text substitutions applied to component configure options and metadata

## Gate Definition

Gates are defined in `gate.kdl` files using the KDL configuration language. See the [gate.kdl Reference](../packaging-guide/gate-kdl-reference.md) for the full specification.

A minimal gate definition:

```kdl
name "userland"
version "0.5.11"
branch "2024.0.0"
publisher "openindiana.org"
```

## How Gates Work

### Intake

A maintainer creates a component within a gate, providing a `package.kdl` recipe, source archives, and any patches. The gate validates the recipe against its policies.

### Integration

Components are reviewed and integrated into the gate's stable branch. Gate-level transforms are applied to normalize packaging across all components.

### Build and Publish

When a build is triggered, the gate provides the context (version, branch, publisher, transforms) that shapes the final IPS packages. Build artifacts are stored in SeaweedFS and can be published to an IPS repository.

## Repository Layout

A typical gate repository follows this structure:

```
gate/
  gate.kdl
  transforms/
    defaults.mogrify
    developer.mogrify
  components/
    library/zlib/
      package.kdl
      patches/
        001-fix-configure.patch
    web/curl/
      package.kdl
      patches/
        000-configure.ac.patch
```

## Use Cases

- **Distribution gates** -- A distribution like OpenIndiana maintains a `userland` gate containing hundreds of components under unified versioning and policy.
- **Project gates** -- A team maintains a gate for their own software stack, publishing to a private IPS repository.
- **Overlay gates** -- A gate that layers additional packages on top of an existing distribution.
