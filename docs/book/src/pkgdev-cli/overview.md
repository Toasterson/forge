# pkgdev CLI Overview

`pkgdev` is the command-line tool for package developers working with Forge. It handles authentication, component management, source downloads, builds, and local repository contexts.

## Usage

```
pkgdev [OPTIONS] <COMMAND>
```

## Global Options

| Option | Description |
|---|---|
| `--gate <PATH>` | Path to the `gate.kdl` file |
| `--workspace <PATH>` | Override the workspace directory |
| `--repo <PATH>` | Override the repository path |
| `--repo-context <NAME>` | Select a named repository context |

## Commands

| Command | Description |
|---|---|
| `auth` | Authentication and key management |
| `forge` | Interact with a Forge server (gates, components) |
| `download` | Download source archives for a component |
| `metadata` | Extract and display component metadata |
| `generate` | Generate schemas, manifests, or reports |
| `create` | Create a new component |
| `edit` | Edit a component locally |
| `build` | Build a component |
| `repo` | Manage local repository contexts |

See the following sections for details on each command group:

- [Authentication](./auth.md)
- [Forge Commands](./forge.md)
- [Building](./build.md)
- [Repository Management](./repo.md)
