# gate.kdl Reference

This is the complete reference for the `gate.kdl` gate definition format.

## Top-level Fields

| Field | Required | Repeatable | Description |
|---|---|---|---|
| `id` | no | no | Stable UUID for the gate |
| `name` | yes | no | Human-readable gate name |
| `version` | yes | no | IPS version string (e.g., `"0.5.11"`) |
| `branch` | yes | no | IPS branch string (e.g., `"2024.0.0"`) |
| `publisher` | yes | no | IPS publisher name |
| `distribution` | no | no | Distribution type block |
| `transform` | no | yes | IPS manifest transform rule |
| `metadata-transform` | no | yes | Metadata text substitution |

### Example

```kdl
id "a1a6a18e-61bf-4d13-b00b-ff543c91e890"
name "userland"
version "0.5.11"
branch "2024.0.0"
publisher "openindiana.org"
```

## Distribution

Sets the output format for packages produced by this gate. Defaults to `"ips"` if omitted.

```kdl
distribution {
    type "ips"
}
```

| Value | Description |
|---|---|
| `"ips"` | Image Packaging System format (default) |
| `"tarball"` or `"tar"` | Compressed tar archive |

## Metadata Transforms

Metadata transforms perform text substitutions on component metadata -- typically configure options and environment variables. They are applied gate-wide before builds.

```kdl
metadata-transform matcher="--libexecdir" replacement="--libexecdir=${LIBEXECDIR}"
metadata-transform matcher="--bindir" replacement="--bindir=${BINDIR}"
metadata-transform matcher="--with-jobs" drop=true
```

| Property | Required | Default | Description |
|---|---|---|---|
| `matcher` | yes | -- | String or regex pattern to match |
| `replacement` | no | `""` | Replacement text (may contain `${VAR}` references) |
| `drop` | no | `false` | If `true`, remove matching entries entirely instead of replacing |

Use cases:

- **Variable expansion** -- Replace hardcoded paths with gate-level variables (e.g., `${LIBEXECDIR}`)
- **Option normalization** -- Ensure consistent configure flags across components
- **Option removal** -- Drop options that are not applicable to the target platform

## Transforms

Transforms modify IPS manifests produced by component builds. They apply gate-wide policy such as default file ownership, facets, and action filtering.

### Legacy Format

For backward compatibility with `pkgmogrify`, transforms can be specified as inline text or included from files:

```kdl
transform "add set name=pkg.fmri value=pkg://$(PUBLISHER)/$(COMPONENT_NAME)@$(VERSION),$(BRANCH)"
transform include="transforms/defaults.mogrify"
```

### KDL Rule Format

The preferred format uses structured rules with selectors and operations:

```kdl
transform {
    rule {
        select action="file" attr="path" pattern="usr/bin/.*"
        op "set" key="owner" value="root"
        op "set" key="group" value="bin"
        op "set" key="mode" value="0555"
    }
    rule {
        select action="file" attr="path" pattern="usr/share/doc/.*"
        op "set" key="facet.doc" value="true"
    }
    rule {
        select action="file" attr="path" pattern="usr/share/man/.*"
        op "set" key="facet.doc.man" value="true"
    }
    rule {
        select action="dir" attr="path" pattern="usr$"
        op "drop"
    }
}
```

#### Selectors

Each rule has one `select` node that determines which IPS actions the rule applies to:

| Property | Description |
|---|---|
| `action` | IPS action type to match: `file`, `link`, `hardlink`, `dir`, etc. |
| `attr` | Attribute name to match against (e.g., `path`, `mode`, `owner`) |
| `pattern` | Regex pattern applied to the attribute value |

#### Operations

Each rule has one or more `op` nodes that modify matching actions:

| Operation | Properties | Description |
|---|---|---|
| `op "set"` | `key`, `value` | Set an attribute to a value (overwrites if exists) |
| `op "delete"` | `key` | Remove an attribute |
| `op "default"` | `key`, `value` | Set an attribute only if not already present |
| `op "drop"` | -- | Remove the entire action from the manifest |

## Complete Example

```kdl
id "a1a6a18e-61bf-4d13-b00b-ff543c91e890"
name "userland"
version "0.5.11"
branch "2024.0.0"
publisher "openindiana.org"

distribution {
    type "ips"
}

metadata-transform matcher="--libexecdir" replacement="--libexecdir=${LIBEXECDIR}"
metadata-transform matcher="--bindir" replacement="--bindir=${BINDIR}"
metadata-transform matcher="--sbindir" replacement="--sbindir=${SBINDIR}"
metadata-transform matcher="--with-jobs" drop=true

transform {
    rule {
        select action="file" attr="path" pattern="usr/bin/.*"
        op "set" key="owner" value="root"
        op "set" key="group" value="bin"
        op "set" key="mode" value="0555"
    }
    rule {
        select action="file" attr="path" pattern="usr/share/man/.*"
        op "set" key="facet.doc.man" value="true"
    }
}

transform include="transforms/cleanup.mogrify"
```
