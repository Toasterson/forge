# Gate KDL Reference

This page is the authoritative reference for authoring a gate.kdl file. A gate describes shared policy and transforms that apply to components in a Gate (repository) and the target distribution(s).

The gate KDL is parsed by crates/gate, mapping 1:1 to the structs in crates/gate/src/lib.rs. Use this reference to write valid files.

## File name

- Conventional name: gate.kdl (or similar). The loader expects the path to the actual KDL file.

## Top-level structure

Canonical form (preferred):

```text
gate {
  id "<uuid>"             // optional
  name "<gate-name>"      // required
  version "<semver>"      // required
  branch "<branch-name>"  // required
  publisher "<string>"    // required

  distribution {
    type "ips" | "tarball"
  }

  // Zero or more transform blocks (see Transforms section)
  transform ...

  // Zero or more metadata transforms
  metadata-transform matcher="..." replacement="..." [drop=true]
}
```

Flat form (also accepted by the loader) looks like the sample below; saving via the library will render the canonical `gate { ... }` form.

## Required fields

- name: string
- version: string
- branch: string
- publisher: string

Optional fields
- id: string (e.g., UUID)
- distribution: block with a single child `type`
- transform: repeated blocks
- metadata-transform: repeated lines

Notes
- While the Gate type has Default values in code, decoding from KDL requires the required fields to be present in your file.

## Distribution block

Purpose: Selects the distribution type for downstream publishing behavior.

Syntax:
- distribution { type "ips" }
- distribution { type "tarball" }

Details:
- Allowed values for type: ips, tarball (aliases: tar)

## Metadata transforms

Purpose: Normalize or filter component metadata values during build (e.g., configure flags, env keys).

Syntax (repeatable):
- metadata-transform matcher="<string>" [replacement="<string>"] [drop=true]

Fields:
- matcher (required): string to match
- replacement (optional, defaults to empty string): replacement value
- drop (optional, boolean, default false): if true, matched metadata entries are removed

Example:
```text
metadata-transform matcher="--bindir" replacement="--bindir=${BINDIR}"
metadata-transform matcher="CFLAGS" drop=true
```

## Transforms

Purpose: Apply policy to manifests (pkg(5)/IPS actions) at gate level.

There are two forms supported by the parser:

1) Legacy textual form (arguments and include):

```text
transform "<legacy mogrify line>" ["another line" ...] include="path/to/file.mog"
```
- Any number of positional arguments are preserved as lines (`to_transform_line`).
- include (optional): path to a legacy include file.

2) KDL AST form with typed rules:

```text
transform {
  rule {
    select action="file|link|hardlink|dir" attr="path|mode|owner|..." pattern="<glob-or-exact>"
    op "set" key="<attr>" value="<val>"
    op "delete" key="<attr>"
    op "drop"
    op "default" key="<attr>" value="<val>"
  }
  // ...more rule blocks
}
```
- rule: groups selectors and operations
- select:
  - action (optional): IPS action to match (file, link, hardlink, dir)
  - attr (optional): attribute name to match (e.g., path)
  - pattern (optional): value matcher for the attr
- op:
  - first argument: operation name (set | delete | drop | default)
  - key (optional): attribute key for applicable operations
  - value (optional): attribute value for applicable operations

Both forms can be mixed by adding multiple transform entries.

## Complete example (canonical)

```text
gate {
  id "a1a6a18e-61bf-4d13-b00b-ff543c91e890"
  name "userland"
  version "0.5.11"
  branch "2024.0.0"
  publisher "openindiana.org"

  distribution { type "ips" }

  // legacy form
  transform "<include transforms/global.mog>"

  // typed form
  transform {
    rule {
      select action="file" attr="path" pattern="usr/bin/*"
      op "set" key="mode" value="0555"
    }
  }

  metadata-transform matcher="--libexecdir" replacement="--libexecdir=${LIBEXECDIR}"
  metadata-transform matcher="--bindir" replacement="--bindir=${BINDIR}"
  metadata-transform matcher="--sbindir" replacement="--sbindir=${SBINDIR}"
  metadata-transform matcher="--mandir" replacement="--mandir=${MANDIR}"
  metadata-transform matcher="--libdir" replacement="--bindir=${LIBDIR}"
  metadata-transform matcher="--with-jobs" drop=true
  metadata-transform matcher="CC" drop=true
  metadata-transform matcher="CXX" drop=true
  metadata-transform matcher="F77" drop=true
  metadata-transform matcher="FC" drop=true
  metadata-transform matcher="FFLAGS" drop=true
  metadata-transform matcher="CFLAGS" drop=true
  metadata-transform matcher="LDFLAGS" drop=true
  metadata-transform matcher="PKG_CONFIG_PATH" drop=true
}
```

## Minimal example (flat form)

This mirrors sample_data/userland-gate.kdl:

```text
id "a1a6a18e-61bf-4d13-b00b-ff543c91e890"
name "userland"
version "0.5.11"
branch "2024.0.0"
publisher "openindiana.org"

metadata-transform matcher="--libexecdir" replacement="--libexecdir=${LIBEXECDIR}"
metadata-transform matcher="--bindir" replacement="--bindir=${BINDIR}"
metadata-transform matcher="--sbindir" replacement="--sbindir=${SBINDIR}"
metadata-transform matcher="--mandir" replacement="--mandir=${MANDIR}"
metadata-transform matcher="--libdir" replacement="--bindir=${LIBDIR}"
metadata-transform matcher="--with-jobs" drop=true
metadata-transform matcher="CC" drop=true
metadata-transform matcher="CXX" drop=true
metadata-transform matcher="F77" drop=true
metadata-transform matcher="FC" drop=true
metadata-transform matcher="FFLAGS" drop=true
metadata-transform matcher="CFLAGS" drop=true
metadata-transform matcher="LDFLAGS" drop=true
metadata-transform matcher="PKG_CONFIG_PATH" drop=true
```

## Mapping to types (for implementers)

- Gate -> crates/gate::Gate
  - id: Option<String>
  - name: String
  - version: String
  - branch: String
  - publisher: String
  - distribution: Option<Distribution>
  - default_transforms: Vec<Transform> (serialized as `transform`)
  - metadata_transforms: Vec<MetadataTransform>
- Distribution.type -> DistributionType: ips | tarball
- Transform
  - legacy: positional arguments as lines, include property
  - typed: children `rule` with `select` and `op`
- MetadataTransform: matcher, replacement (default ""), drop (default false)

Tip: When in doubt, run the library to load and then save your file; it will normalize to the canonical `gate { ... }` structure.

## Related
- Gates (concept): Gates.md
- Distributions: Distributions.md
- Declarative packages: Declarative-Packages.md
- Manifests: Manifests.md