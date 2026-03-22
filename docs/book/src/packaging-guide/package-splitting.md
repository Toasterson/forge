# Package Splitting

A single component build can produce multiple IPS packages. This is common for libraries, where the shared objects go into a runtime package and headers, static libraries, and pkg-config files go into a developer package.

## How It Works

Package splitting is controlled by `package` sections in `package.kdl`. Each `package` block defines a set of regex patterns that match files from the build output. Files matching a pattern are assigned to that package.

The default (unnamed) `package` block captures the main package. Named `package` blocks create sub-packages with the given FMRI suffix.

## Basic Split: Library + Developer

```kdl
name "library/zlib"
summary "The zlib Compression Library"
version "1.3.1"

source {
    archive "https://zlib.net/zlib-1.3.1.tar.xz" \
        sha256="abc123..."
}

build {
    configure {
        option "--prefix=/usr"
        option "--shared"
    }
}

// Main package: shared libraries and binaries
package {
    file path="usr/lib/.*\\.so\\..*"
    file path="usr/bin/.*"
    link path="usr/lib/.*\\.so$"
}

// Developer package: headers, static libs, pkg-config
package "developer/library/zlib" {
    file path="usr/include/.*"
    file path="usr/lib/.*\\.a$"
    file path="usr/lib/pkgconfig/.*\\.pc$"
    file path="usr/share/man/man3/.*"
}
```

This produces two packages:

- `library/zlib` -- contains `libz.so.1`, `libz.so.1.3.1`, and symlinks
- `developer/library/zlib` -- contains `zlib.h`, `libz.a`, `zlib.pc`, and man3 pages

## Complex Split: FFmpeg

A multimedia library like FFmpeg can be split into many sub-packages:

```kdl
name "video/ffmpeg-7"
version "7.0.2"

source {
    archive "https://ffmpeg.org/releases/ffmpeg-7.0.2.tar.xz" \
        sha256="abc123..."
    patch "ffmpeg-7-smb.patch" drop-directories=1
    patch "ffmpeg-7-srt.patch" drop-directories=1
}

build {
    configure {
        option "--prefix=/usr"
        option "--enable-shared"
        option "--disable-static"
        option "--enable-gpl"
        option "--enable-libx264"
        option "--enable-libx265"
    }
}

// Main package: shared libraries and binaries
package {
    file path="usr/bin/.*"
    file path="usr/lib/.*\\.so\\..*"
    file path="usr/share/man/man1/.*"
    file path="usr/share/ffmpeg/.*"
    link path="usr/lib/.*\\.so$"
}

// Developer package: headers and pkg-config
package "developer/ffmpeg-7" {
    file path="usr/include/.*"
    file path="usr/lib/pkgconfig/.*\\.pc$"
    file path="usr/share/man/man3/.*"
}
```

## Pattern Matching

The `path` property on `file`, `link`, and `hardlinks` nodes uses regular expressions. Patterns are matched against the full installed path (without a leading `/`).

Common patterns:

| Pattern | Matches |
|---|---|
| `usr/bin/.*` | All files under `usr/bin/` |
| `usr/lib/.*\\.so\\..*` | Versioned shared libraries (e.g., `libfoo.so.1.2.3`) |
| `usr/lib/.*\\.so$` | Unversioned shared library symlinks |
| `usr/lib/.*\\.a$` | Static libraries |
| `usr/include/.*` | Header files |
| `usr/lib/pkgconfig/.*\\.pc$` | pkg-config files |
| `usr/share/man/man1/.*` | Section 1 man pages |
| `usr/share/man/man3/.*` | Section 3 man pages |

## Attribute Overrides

Package nodes can override IPS manifest attributes for matched files:

```kdl
package {
    file path="usr/bin/.*" mode="0555" owner="root" group="bin"
    file path="etc/.*" mode="0644" owner="root" group="sys"
}
```

## Tips

- The default package (no name argument) should contain the runtime files users need
- Developer packages conventionally use a `developer/` prefix in their FMRI
- Files not matched by any pattern fall through to the main package
- Order matters when patterns overlap -- the first matching `package` block wins
- Use `link` for symbolic links and `hardlinks` for hard links; don't use `file` for these
