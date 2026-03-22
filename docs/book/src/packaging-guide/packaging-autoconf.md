# Packaging Autoconf Software

Most C and C++ projects on illumos use GNU Autotools (`autoconf`/`automake`). This is the most common build system you will encounter and is handled by the `configure` keyword in `package.kdl`.

## Basic Structure

```kdl
name "library/example"
summary "An example autoconf library"
version "1.0.0"

source {
    archive "https://example.com/example-1.0.0.tar.gz" \
        sha256="abc123..."
}

build {
    configure {
        option "--prefix=/usr"
        option "--enable-shared"
        option "--disable-static"
    }
}
```

Forge generates the equivalent of:

```bash
./configure --prefix=/usr --enable-shared --disable-static
make
make install DESTDIR=$PROTO_DIR
```

## Compiler and Linker Selection

Override the default compiler and linker:

```kdl
build {
    configure {
        compiler "gcc"
        linker "/usr/bin/ld"

        option "--prefix=/usr"
    }
}
```

## Compiler Flags

Add flags to environment variables passed to configure:

```kdl
build {
    configure {
        option "--prefix=/usr"

        flag "-m64" name="CFLAGS"
        flag "-m64" name="CXXFLAGS"
        flag "-m64" name="LDFLAGS"
        flag "-I/usr/include/glib-2.0" name="CPPFLAGS"
        flag "-L/usr/lib/amd64" name="LDFLAGS"
        flag "-O2" name="CFLAGS"
    }
}
```

Flags with the same `name` are accumulated and passed together.

## Special Options

### Large File Support

Enable large file support (adds `_FILE_OFFSET_BITS=64` and related flags):

```kdl
build {
    configure {
        enable-large-files
        option "--prefix=/usr"
    }
}
```

### Disabling DESTDIR

Some configure scripts do not support `DESTDIR`. Disable the DESTDIR-related configure option:

```kdl
build {
    configure {
        disable-destdir-option
        option "--prefix=/usr"
    }
}
```

## Real-World Example: curl

```kdl
name "web/curl"
project-name "curl"

metadata {
    anitya-id "381"
    repology-id "curl"
}

classification "System/Libraries"
summary "The CURL Network Utility and Library"
license "CURL"
license-file "COPYING"
version "8.6.0"
project-url "https://curl.se"
maintainer "The OpenIndiana Maintainers"

source {
    archive "https://curl.haxx.se/download/curl-8.6.0.tar.xz" \
        sha256="3ccd55d91af9516539df80625f818c734dc6f2ecf9bada33c76765e99121db15"
    patch "000-configure.ac.patch"
    patch "005-libcurl.pc.in.patch"
}

build {
    configure {
        option "--prefix=/usr"
        option "--mandir=/usr/share/man"
        option "--enable-shared"
        option "--disable-static"
        option "--enable-ipv6"
        option "--enable-http"
        option "--enable-ftp"
        option "--enable-file"
        option "--with-ssl=/usr/openssl/3.1"
        option "--with-zlib=/usr"
        option "--with-ca-bundle=/etc/certs/ca-certificates.crt"

        flag "-m64" name="CFLAGS"
        flag "-m64" name="LDFLAGS"
        flag "-I/usr/openssl/3.1/include" name="CPPFLAGS"
        flag "-L/usr/openssl/3.1/lib/amd64" name="LDFLAGS"
    }
}

dependency "library/zlib" dev=true kind="require"
dependency "library/openssl-31" dev=true kind="require"
dependency "library/libssh2" dev=true kind="require"
dependency "library/nghttp2" dev=true kind="require"
dependency "system/library" kind="require"
```

## Tips

- Always specify `--prefix=/usr` for system packages
- Use `--disable-static` unless static libraries are specifically needed
- Set `--mandir=/usr/share/man` so man pages land in the right place
- Add `CFLAGS` and `LDFLAGS` with `-m64` for 64-bit builds
- Specify library and include paths explicitly when depending on packages installed in non-standard locations (e.g., `/usr/openssl/3.1`)
