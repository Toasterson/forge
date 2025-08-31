# Gate

A Gate in Forge is the primary, version-controlled entry point for sources and policies used to build and publish packages into one or more distributions. It centralizes shared rules (like transforms, branch versions, and toolchain policy) while keeping component-level packaging logic inside each component.

## Background

The term Gate originates at Sun Microsystems. It referred to the place where changes were integrated for building the full operating system and its bundled software. In illumos and downstreams, the Gate continues to represent the stable, curated source tree where integration happens, with forking and downstream divergence embraced (see “Fork yeah!”).

## What a Gate contains

A Gate typically includes:
- Components: Declarative packages (package.kdl and packaging assets) grouped under a common repository.
- Central policy and transforms: Reusable, distribution-scoped or gate-scoped transforms that unify metadata, license headers, file attributes, and packaging conventions.
- Branch and version policy: Default version/revision rules, branch naming, and integration cadence (e.g., weekly gates, quarterly release branches).
- Tooling and CI integration: Scripts or configuration to run validation, builds, and publishing pipelines for the gate as a whole.
- Metadata and configuration: Files like forged.toml or equivalent that describe default behaviors, paths, or build matrix settings for the gate.

## Relationships to other concepts

- Components: Components live inside a gate and define how to fetch, build, and package a single piece of software. The gate provides shared policy; components provide per-package detail.
- Distributions: A distribution consumes one or more gates. Gates can carry distribution-specific transforms and policy. A distribution selects which gates and branches to incorporate, determining what ultimately ships.
- Manifests: Build and publish outputs (like IPS manifests) are produced from components under the gate, influenced by gate-level transforms.

## Scope, inheritance, and overrides

- Gate-level policy applies by default to all components in the gate.
- Distributions may override or extend gate policy when importing from a gate (e.g., adding distro-specific transforms).
- Components can opt out of some policies where supported (for example, by setting specific component metadata), but in general the gate seeks consistency by making central rules the default.

## Typical repository layout (example)

This is an illustrative layout; actual projects vary:

- components/
  - component-a/
    - package.kdl
    - patches/
    - files/
  - component-b/
- transforms/
  - global.mog
  - license-normalization.mog
- tools/
  - ci/
  - scripts/
- forged.toml
- docs/

## Workflows

- Intake: New or updated components are proposed via PRs against the gate.
- Review: Automated checks (linting, schema validation, build tests) and human review ensure policy compliance.
- Integration: Approved changes are merged into the gate’s integration branch.
- Build and test: CI builds components, applies gate-level transforms, and validates manifests.
- Publish: Distributions pull from a known-good gate branch/tag to produce and publish packages.

## Use cases

- Centralize policy and transforms for a family of components.
- Maintain a stable integration branch that downstream distributions can consume.
- Coordinate larger transitions (e.g., toolchain updates, FHS layout changes) consistently across many components.

## Best practices

- Keep gate-level transforms small, composable, and well-documented.
- Prefer declarative configuration (e.g., KDL, TOML) over imperative scripts where possible.
- Use branches/tags to communicate stability (e.g., integration, release/X.Y).
- Automate validation in CI to catch policy violations early.

## Related resources

- Declarative Packages: Declarative-Packages.md
- Components: Components.md
- Distributions: Distributions.md
- Manifests: Manifests.md
