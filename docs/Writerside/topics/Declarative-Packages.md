# Declarative Packages

This article explains the basics of declarative packages and how they work in Forge. Declarative packages let you describe how to fetch sources, build software, and shape deliverables using a readable KDL file. Forge parses that KDL into a strongly-typed component model and executes the build/package workflow from it.

A declarative package is:
- a package.kdl file stored alongside your sources and packaging assets;
- a structured description of inputs (sources), build steps, dependencies, and packaging rules;
- a stable interface that maps directly to Forge’s internal data structures (Recipe, BuildSection, Source nodes, etc.).

By using declarative packages, maintainers gain:
- repeatable builds that are easy to review and diff;
- a single source of truth for build and packaging logic;
- portability across environments through a stable schema.

To use declarative packages, you create a package.kdl file in your component directory. Forge parses it into a Recipe (see Components) and drives the rest of the pipeline from there.

## Background

Forge adopts a “describe, don’t script” approach for common packaging tasks. Instead of imperative shell scripts, most behavior can be expressed via KDL nodes and properties. This makes packages easier to audit and maintain while still allowing escape hatches when custom scripting is necessary.

## The package.kdl structure

At a high level, the KDL document maps to the Recipe type. Key sections are:

- name: The component’s canonical name.
- metadata: Arbitrary key/value metadata about the component.
- project-name, classification, summary, license, license-file, prefix, version, revision, project-url: Optional descriptors.
- maintainer: One or more maintainers.
- source: One or more sources to fetch/unpack (archive, git, file, directory, patch, overlay).
- dependency: Component/package dependencies, with kind and whether they are dev-only.
- build: Build instructions. Choose configure/script/meson/cmake per section.
- package: Packaging transformations that shape files, links, and hardlinks into output packages.

These sections are parsed into the following internal types:
- Recipe: Top-level document.
- SourceSection -> SourceNode: One of archive/git/file/directory/patch/overlay.
- Dependency: name, kind (require/incorporate/optional), dev (bool).
- BuildSection: Optional source selector plus exactly one of configure/script/meson/cmake.
- PackageSection: Optional name, with file/link/hardlinks items of TransformNode.

## Example: a minimal yet complete package.kdl

```text
package {
  name "zlib"
  version "1.2.13"
  revision "0"
  summary "Compression library"
  license "Zlib"
  project-url "https://zlib.net"
  maintainer "oi-userland@openindiana.org"

  metadata {
    upstream "zlib.net"
    category "library"
  }

  source {
    archive "https://zlib.net/zlib-1.2.13.tar.xz" sha256 "abcd..."
  }

  dependency "system/library" kind="require"

  build {
    configure {
      option "--shared"
      flag "-O2"
      enable-large-files
    }
  }

  package "library" {
    file src="usr/lib"
    link src="usr/lib/libz.so.1" target="usr/lib/64/libz.so.1"
  }
}
```

Notes:
- name/version/revision are used to identify the component (Display format: name@version-revision). If version or revision are omitted, defaults are used internally (0.1.0 and 0 respectively).
- metadata stores arbitrary key/value pairs.
- source can include multiple entries and types. Each child node maps to a specific SourceNode variant.
- build picks one approach per section: configure, script, meson, or cmake. If no builder is specified, a no-build node is emitted.
- package sections can be repeated to split deliverables. Items are generic TransformNode entries that become packaging rules.

## Sources

Supported source nodes under a source section:
- archive "URL" [sha512 "…"] [sha256 "…"] [signature-url "…"] [singature-url-extension "…"]
- git "REPO" [branch "…"] [tag "…"] [archive true|false] [must-stay-as-repo true|false] [directory "subdir"]
- file "bundle/path" ["target/path"]
- directory "bundle/dir" ["target/dir"]
- patch "bundle/patch" [drop-directories N]
- overlay "bundle/dir"

These map to ArchiveSource, GitSource, FileSource, DirectorySource, PatchSource, and OverlaySource respectively.

## Dependencies

Each dependency node declares a relation:
- dependency "component-name" kind "require|incorporate|optional" [dev true]

Kinds:
- require (default)
- incorporate (bring in contents without hard requirement semantics)
- optional

## Build sections

Each build section may target a specific source by name via a leading argument, e.g. build "libfoo" { … }.

Supported builders:
- configure: options, flags, compiler/linker selection, enable-large-files, disable-destdir-option.
- script: arbitrary script steps and install directives.
- meson, cmake: string-valued selectors are supported; details are handled elsewhere in Forge.

If no builder is present, Forge considers it a no-build section.

## Packaging sections

Use package sections to describe how files are turned into deliverables:
- package ["name"] { … }
  - file/link/hardlinks entries are generic transformations with selectors expressed as key/value pairs.

Internally, these become PackageSection and TransformNode entries.

## Use cases

- Simple upstream tarball with autotools: archive + configure + package files.
- VCS-based builds: git source with branch/tag and either configure/meson/cmake.
- Non-building assets: directory or file sources with package-only rules.
- Split outputs: multiple package sections to create runtime, devel, and docs variants.

## Related concepts

- Components: docs/Writerside/topics/Components.md
- Manifests: docs/Writerside/topics/Manifests.md
- Gates and Distributions: docs/Writerside/topics/Gates.md, docs/Writerside/topics/Distributions.md

If you’re starting a new package, use declarative packages as your default. Reserve custom scripting for cases where declarative options aren’t sufficient.