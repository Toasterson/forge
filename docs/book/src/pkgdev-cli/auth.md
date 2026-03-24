# Authentication

Forge uses OIDC for authentication. The `pkgdev auth` commands manage login, SSH key registration, and token status.

## Login

Authenticate with a Forge server using the OIDC device authorization flow:

```bash
pkgdev auth login --host https://forge.example.com
```

This contacts the Forge server to discover the OIDC provider, then starts the device flow:

1. Displays a URL and a code
2. You visit the URL in a browser and enter the code
3. After you authorize, pkgdev receives an access token

The token is stored locally at `~/.local/share/pkgdev/tokens.json` with `0600` permissions. The host is automatically set as the default context for subsequent commands.

## Add SSH Key

Add an SSH public key to your OIDC-authenticated account:

```bash
pkgdev auth add-key --public-key ~/.ssh/id_ed25519.pub --key-id laptop
```

Requires a prior `auth login`. The `--key-id` is a human-readable label for the key (default: `"default"`). The `--host` flag is optional if you only have one login or a selected context.

## Token Status

Check the status of stored tokens:

```bash
pkgdev auth status
```

Shows token expiry, whether a refresh token is available, and the OIDC issuer for each stored host.

## Logout

Remove stored tokens for a host:

```bash
pkgdev auth logout --host https://forge.example.com
```

## Listing Contexts

View all authentication contexts:

```bash
pkgdev auth list
```

## Selecting a Default Context

Switch the active context:

```bash
pkgdev auth select \
  --host https://forge.example.com \
  --actor-id myuser
```

The selected context is used by default for all `pkgdev forge` commands.
