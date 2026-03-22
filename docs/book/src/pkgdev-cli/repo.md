# Repository Management

Repository contexts let you manage multiple local working directories for different gates or projects.

## List Contexts

```bash
pkgdev repo list
```

## Create a Context

```bash
pkgdev repo create my-gate
pkgdev repo create my-gate /path/to/gate
```

If no path is given, a default location is used.

## Select a Context

Set the active context for subsequent commands:

```bash
pkgdev repo select my-gate
```

The selected context determines which gate and workspace `pkgdev` operates on by default.

## Delete a Context

```bash
pkgdev repo delete my-gate
```

This removes the context reference, not the files on disk.

## Using Contexts

Once a context is selected, you can omit `--gate` and `--workspace` flags:

```bash
pkgdev repo select userland
pkgdev forge component list --host http://localhost:50051
pkgdev build --component components/library/zlib
```

Override the context for a single command with `--repo-context`:

```bash
pkgdev --repo-context other-gate forge gate list --host http://localhost:50051
```
