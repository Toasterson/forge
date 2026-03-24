# Packaging Services

This guide covers how to package daemon services on illumos using Forge. Services require additional packaging steps beyond simple binaries: system accounts, SMF integration, configuration file preservation, and directory ownership.

## Overview

A typical service package needs to:

1. Create system users and groups for the daemon to run as
2. Deliver SMF manifests and method scripts so the service is managed by `svc.startd`
3. Deliver configuration files and mark them as preserved across upgrades
4. Create data directories with the correct ownership
5. Ensure SMF picks up new manifests automatically

Forge's `package.kdl` format provides dedicated features for each of these.

## Creating System Users and Groups

Services should run under a dedicated system account, not root. Declare `group` and `user` as top-level nodes:

```kdl
group "myservice" gid=10050
user "myservice" uid=10050 group="myservice" \
    home="/var/lib/myservice" \
    shell="/usr/bin/false" \
    description="My Service daemon"
```

IPS processes these actions before file delivery, so the accounts are available when directories and files are created with the correct ownership.

Key points:
- Choose a GID/UID in a range that does not conflict with system accounts (typically above 10000 for site-specific services)
- Set `shell="/usr/bin/false"` to prevent interactive login
- Set `home` to the service's data directory
- The `description` field appears in `/etc/passwd` as the GECOS field

## Delivering SMF Manifests and Method Scripts

illumos uses the Service Management Facility (SMF) to manage daemons. You need to deliver two files:

1. **SMF manifest** (XML): Declares the service, its dependencies, properties, and method references
2. **Method script** (shell): The script SMF calls to start, stop, and refresh the service

Use the `files {}` section to install these from your component directory:

```kdl
files {
    install "smf/myservice.xml" "lib/svc/manifest/application/myservice.xml"
    install "smf/myservice-method" "opt/myservice/lib/svc/method/myservice-method" mode="0555"
}
```

The SMF manifest goes under `lib/svc/manifest/` (the standard location for site manifests). The method script should be executable (`mode="0555"`).

In the `package {}` section, tell IPS to trigger `manifest-import` when the manifest is installed or updated:

```kdl
package {
    file path="lib/svc/manifest/.*" restart-fmri="svc:/system/manifest-import:default"
}
```

This ensures `svccfg import` is run automatically after package installation, registering or updating the service.

## Configuration File Preservation

Configuration files should be marked with `preserve="true"` so that user edits are not lost during package upgrades. IPS will keep the user's version and save the new packaged version alongside it for reference.

```kdl
files {
    install "files/myservice.toml" "etc/myservice/myservice.toml"
}

package {
    file path="etc/myservice/.*" preserve="true" mode="0640" owner="root" group="myservice"
}
```

The `mode="0640" owner="root" group="myservice"` pattern gives root write access and allows the service user (via group membership) to read the config, but prevents other users from reading potentially sensitive settings.

## Directory Ownership

Service data directories must be owned by the service user so the daemon can write to them. Use `dir` entries in the `package {}` section:

```kdl
package {
    dir path="var/lib/myservice" owner="myservice" group="myservice" mode="0755"
    dir path="var/lib/myservice/data" owner="myservice" group="myservice" mode="0755"
    dir path="var/log/myservice" owner="myservice" group="myservice" mode="0755"
    dir path="etc/myservice" owner="root" group="myservice" mode="0755"
}
```

Directories that hold secrets or credentials should use a more restrictive mode:

```kdl
package {
    dir path="var/lib/myservice/tls" owner="myservice" group="myservice" mode="0700"
}
```

## Complete Example: Packaging a Key-Value Store Service

This example packages a hypothetical key-value store daemon called `kvsd`, built from a Rust project with SMF integration.

### Component directory layout

```
components/database/kvsd/
    package.kdl
    smf/
        kvsd.xml           # SMF manifest
        kvsd-method        # SMF method script
    files/
        kvsd.toml          # Default configuration
```

### package.kdl

```kdl
name "database/kvsd"
project-name "kvsd"
classification "System/Databases"
summary "A lightweight key-value store daemon"
license "MIT"
license-file "LICENSE"
version "1.0.0"
project-url "https://github.com/example/kvsd"
maintainer "The OpenIndiana Maintainers"

metadata {
    repology-id "kvsd"
}

source {
    archive "https://github.com/example/kvsd/archive/v1.0.0.tar.gz" \
        sha256="abc123..."
}

build {
    cargo {
        packages "kvsd"
        install-root "/opt/kvsd"
    }
}

files {
    install "smf/kvsd.xml" "lib/svc/manifest/application/kvsd.xml"
    install "smf/kvsd-method" "opt/kvsd/lib/svc/method/kvsd-method" mode="0555"
    install "files/kvsd.toml" "etc/kvsd/kvsd.toml"
}

group "kvsd" gid=10050
user "kvsd" uid=10050 group="kvsd" \
    home="/var/lib/kvsd" \
    shell="/usr/bin/false" \
    description="KVS daemon"

dependency "system/library" kind="require"
dependency "system/library/gcc-14-runtime" kind="require"

package {
    // Binaries
    file path="opt/kvsd/bin/.*"

    // SMF integration
    file path="opt/kvsd/lib/svc/method/.*"
    file path="lib/svc/manifest/.*" restart-fmri="svc:/system/manifest-import:default"

    // Configuration (preserved across upgrades)
    file path="etc/kvsd/.*" preserve="true" mode="0640" owner="root" group="kvsd"

    // Directories with correct ownership
    dir path="var/lib/kvsd" owner="kvsd" group="kvsd" mode="0755"
    dir path="var/lib/kvsd/data" owner="kvsd" group="kvsd" mode="0755"
    dir path="var/log/kvsd" owner="kvsd" group="kvsd" mode="0755"
    dir path="etc/kvsd" owner="root" group="kvsd" mode="0755"
}
```

### What this achieves

After `pkg install database/kvsd`:

1. The `kvsd` group and user are created (GID/UID 10050)
2. The binary is installed at `/opt/kvsd/bin/kvsd`
3. The SMF manifest is imported, registering `svc:/application/kvsd:default`
4. The method script at `/opt/kvsd/lib/svc/method/kvsd-method` is executable
5. The default config at `/etc/kvsd/kvsd.toml` is owned by `root:kvsd` with mode `0640`
6. Data directories under `/var/lib/kvsd/` are owned by the `kvsd` user
7. On upgrade, the user's configuration edits are preserved

The service can then be enabled with:

```
# svcadm enable kvsd
```

## Tips

- Always create the group before the user (IPS processes actions in dependency order, but listing group first makes the recipe clearer)
- Use `/opt/<service>` as the install root for third-party services to avoid polluting `/usr`
- Keep method scripts simple -- they should source `/lib/svc/share/smf_include.sh` and call the binary with appropriate flags
- Test SMF manifests with `svccfg validate smf/myservice.xml` before packaging
- For services that need TLS certificates, create a dedicated directory with mode `0700`
- Mark all files under `/etc/` as `preserve="true"` to respect administrator customizations
