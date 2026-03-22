# Packaging Perl Software

Perl modules from CPAN are typically built with `Makefile.PL` or `Build.PL`. Use the `script` build type.

## Makefile.PL Modules

```kdl
name "library/perl/json"
summary "JSON parsing and generation for Perl"
version "4.10"
license "Artistic-2.0"
project-url "https://metacpan.org/pod/JSON"

source {
    archive "https://cpan.metacpan.org/authors/.../JSON-4.10.tar.gz" \
        sha256="abc123..."
}

build {
    script {
        script "build-perl.sh" prototype-dir="proto"
    }
}

dependency "runtime/perl-536" kind="require"
dependency "system/library" kind="require"
```

Build script for `Makefile.PL` modules:

```bash
#!/bin/bash
set -e
perl Makefile.PL INSTALLDIRS=vendor
make
make install DESTDIR=$PROTO_DIR
```

## Build.PL Modules

For modules using `Module::Build`:

```bash
#!/bin/bash
set -e
perl Build.PL --installdirs=vendor
./Build
./Build install --destdir=$PROTO_DIR
```

## Tips

- Use `INSTALLDIRS=vendor` to install under the vendor Perl directories
- Pin the Perl version in dependencies (e.g., `runtime/perl-536`)
- Perl XS modules (with C code) need `developer/build/make` as a build dependency
- Consider using the `install` node in the `script` block for fine-grained control over installed files
