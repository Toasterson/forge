# Packaging Python Software

Python packages can be built using the `script` build type with appropriate setup commands, or via `configure` when using autotools wrappers.

## Using a Build Script

Most Python packages use `setup.py`, `pyproject.toml`, or similar. Use the `script` build type:

```kdl
name "library/python/requests"
summary "HTTP library for Python"
version "2.31.0"
license "Apache-2.0"
project-url "https://requests.readthedocs.io/"

source {
    archive "https://files.pythonhosted.org/packages/.../requests-2.31.0.tar.gz" \
        sha256="abc123..."
}

build {
    script {
        script "build-python.sh" prototype-dir="proto"
    }
}

dependency "runtime/python-312" kind="require"
dependency "library/python/urllib3" kind="require"
dependency "library/python/certifi" kind="require"
```

The build script (`build-python.sh`) handles the Python-specific build and install steps:

```bash
#!/bin/bash
set -e
python3.12 -m pip install --no-deps --prefix=/usr --root=$PROTO_DIR .
```

## Multiple Python Versions

If you need to build for multiple Python versions, use separate build blocks:

```kdl
build "python3.11" {
    script {
        script "build-py311.sh" prototype-dir="proto"
    }
}

build "python3.12" {
    script {
        script "build-py312.sh" prototype-dir="proto"
    }
}
```

## Tips

- Always pin the Python version in dependencies (e.g., `runtime/python-312`, not `runtime/python`)
- Use `--no-deps` when installing with pip to avoid pulling in vendored dependencies
- Place the build script in the component directory alongside `package.kdl`
- Consider splitting into versioned packages if supporting multiple Python versions
