# Authentication

Forge uses Ed25519 SSH key-based authentication. The `pkgdev auth` commands manage registration, login, and context selection.

## Registration

Register a new actor on a Forge instance:

```bash
pkgdev auth register \
  --host http://localhost:50051 \
  --actor-id myuser \
  --email user@example.com \
  --public-key ~/.ssh/id_ed25519.pub
```

This sends your public key to the server. The server returns an encrypted challenge envelope.

## Confirmation

Confirm your registration by decrypting the challenge:

```bash
pkgdev auth confirm \
  --host http://localhost:50051 \
  --actor-id myuser \
  --envelope <ENCRYPTED_ENVELOPE> \
  --identity ~/.ssh/id_ed25519 \
  --login \
  --select
```

The `--login` flag marks this context as logged in. The `--select` flag makes it the default context.

## Login

Mark an existing registration as active:

```bash
pkgdev auth login \
  --host http://localhost:50051 \
  --actor-id myuser \
  --select
```

## Listing Contexts

View all authentication contexts:

```bash
pkgdev auth list
```

Filter by host:

```bash
pkgdev auth list --host http://localhost:50051
```

## Selecting a Default Context

Switch the active context:

```bash
pkgdev auth select \
  --host http://localhost:50051 \
  --actor-id myuser
```

The selected context is used by default for all `pkgdev forge` commands.
