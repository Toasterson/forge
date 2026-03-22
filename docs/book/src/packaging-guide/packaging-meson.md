# Packaging Meson Software

Meson is a modern build system focused on speed and ease of use. Many GNOME and freedesktop.org projects use Meson. Forge handles Meson projects with the `meson` keyword.

## Basic Structure

```kdl
name "library/example"
summary "A Meson-based library"
version "3.0.0"

source {
    archive "https://example.com/example-3.0.0.tar.xz" \
        sha256="789abc..."
}

build {
    meson "--prefix=/usr" \
          "--buildtype=release" \
          "--default-library=shared"
}
```

Arguments to `meson` are passed directly to `meson setup`. Forge handles the build directory, compilation with `ninja`, and installation.

## Common Meson Options

```kdl
build {
    meson "--prefix=/usr" \
          "--libdir=lib" \
          "--mandir=share/man" \
          "--buildtype=release" \
          "--default-library=shared" \
          "--auto-features=enabled" \
          "-Dtests=false" \
          "-Ddocs=false"
}
```

## Example: A Desktop Library

```kdl
name "library/pango"
summary "Framework for layout and rendering of internationalized text"
version "1.52.0"
project-url "https://pango.gnome.org/"

source {
    archive "https://download.gnome.org/sources/pango/1.52/pango-1.52.0.tar.xz" \
        sha256="abc123..."
}

build {
    meson "--prefix=/usr" \
          "--buildtype=release" \
          "--default-library=shared" \
          "-Dintrospection=enabled" \
          "-Dgtk_doc=false"
}

dependency "library/glib2" dev=true kind="require"
dependency "library/cairo" dev=true kind="require"
dependency "library/freetype2" dev=true kind="require"
dependency "library/harfbuzz" dev=true kind="require"
dependency "system/library" kind="require"
```

## Tips

- Use `--buildtype=release` for optimized, production-ready builds
- Set `--default-library=shared` to produce `.so` files
- Meson uses `-D` for project-specific options (e.g., `-Dtests=false`)
- Use `--libdir=lib` to avoid `lib/x86_64-*` paths
- Meson wraps are not used in packaging -- dependencies come from the system
