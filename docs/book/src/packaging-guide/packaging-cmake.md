# Packaging CMake Software

CMake is a cross-platform build system generator commonly used by C and C++ projects. Forge handles CMake projects with the `cmake` keyword.

## Basic Structure

```kdl
name "library/example"
summary "A CMake-based library"
version "2.0.0"

source {
    archive "https://example.com/example-2.0.0.tar.gz" \
        sha256="def456..."
}

build {
    cmake "-DCMAKE_INSTALL_PREFIX=/usr" \
          "-DBUILD_SHARED_LIBS=ON" \
          "-DCMAKE_BUILD_TYPE=Release"
}
```

Arguments to `cmake` are passed directly to the `cmake` invocation. Forge handles the out-of-source build directory and installation steps.

## Common CMake Options

```kdl
build {
    cmake "-DCMAKE_INSTALL_PREFIX=/usr" \
          "-DCMAKE_INSTALL_LIBDIR=lib" \
          "-DCMAKE_INSTALL_MANDIR=share/man" \
          "-DBUILD_SHARED_LIBS=ON" \
          "-DBUILD_TESTING=OFF" \
          "-DCMAKE_BUILD_TYPE=Release" \
          "-DCMAKE_C_COMPILER=gcc" \
          "-DCMAKE_CXX_COMPILER=g++"
}
```

## Example: A JSON Library

```kdl
name "library/json-c"
summary "A JSON implementation in C"
version "0.17"
project-url "https://github.com/json-c/json-c"

source {
    archive "https://github.com/json-c/json-c/archive/json-c-0.17.tar.gz" \
        sha256="abc123..."
    patch "001-fix-cmake.patch"
}

build {
    cmake "-DCMAKE_INSTALL_PREFIX=/usr" \
          "-DBUILD_SHARED_LIBS=ON" \
          "-DCMAKE_BUILD_TYPE=Release" \
          "-DBUILD_TESTING=OFF"
}

dependency "system/library" kind="require"
```

## Tips

- Always set `-DCMAKE_INSTALL_PREFIX=/usr` for system packages
- Use `-DBUILD_SHARED_LIBS=ON` to produce shared libraries
- Set `-DCMAKE_BUILD_TYPE=Release` for optimized builds
- Disable testing with `-DBUILD_TESTING=OFF` to speed up packaging builds
- Use `-DCMAKE_INSTALL_LIBDIR=lib` to avoid `lib64` on some platforms
