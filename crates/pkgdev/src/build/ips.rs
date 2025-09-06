#[cfg(not(feature = "libips"))]
use crate::sources::derive_source_name;
use component::{Component, SourceNode, TransformNode};
#[cfg(not(feature = "libips"))]
use fs_extra::file::write_all;
use gate::Gate;
#[cfg(not(feature = "libips"))]
use microtemplate::render;
#[cfg(not(feature = "libips"))]
use microtemplate::Substitutions;
use miette::{IntoDiagnostic, Result, WrapErr};
use std::fmt::{Display, Formatter};
#[cfg(not(feature = "libips"))]
use std::fs::File;
use std::path::PathBuf;
#[cfg(not(feature = "libips"))]
use std::process::{Command, Stdio};
use workspace::Workspace;

fn metadata_fmri(pkg: &Component) -> Option<String> {
    if let Some(meta) = &pkg.recipe.metadata {
        for item in &meta.0 {
            if item.name == "fmri" && !item.value.is_empty() {
                return Some(item.value.clone());
            }
        }
    }
    None
}

fn prefixed_name(pkg: &Component, original: String) -> String {
    if let Some(prefix) = metadata_fmri(pkg) {
        let prefix = prefix.trim_end_matches('/');
        // If the prefix already contains the original as the last segment, return the prefix itself
        if prefix.ends_with(&format!("/{}", original)) || original == prefix {
            prefix.to_string()
        } else if original.starts_with(&format!("{}/", prefix)) {
            original
        } else {
            format!("{}/{}", prefix, original)
        }
    } else {
        original
    }
}

#[cfg(not(feature = "libips"))]
const DEFAULT_IPS_TEMPLATE: &str = r#"
#
# This file and its contents are supplied under the terms of the
# Common Development and Distribution License ("CDDL"), version 1.0.
# You may only use this file in accordance with the terms of version
# 1.0 of the CDDL.
#
# A full copy of the text of the CDDL should have accompanied this
# source.  A copy of the CDDL is also available via the Internet at
# http://www.illumos.org/license/CDDL.
#

#
# Copyright 2024 OpenIndiana Maintainers
#

set name=pkg.fmri value=pkg:/{name}@{version},{build_version}-{branch_version}.{revision}
set name=pkg.summary value="{summary}"
set name=info.classification value="org.opensolaris.category.2008:{classification}"
set name=info.upstream-url value="{project_url}"
set name=info.source-url value="{source_url}"

license {license_file_name} license='{license_name}'

"#;
//TODO implement ips component version formatter. build_num (year)

#[cfg(not(feature = "libips"))]
#[derive(Substitutions)]
struct StringInterpolationVars<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub build_version: &'a str,
    pub branch_version: &'a str,
    pub revision: &'a str,
    pub summary: &'a str,
    pub classification: &'a str,
    pub project_url: &'a str,
    pub source_url: &'a str,
    pub license_file_name: &'a str,
    pub license_name: &'a str,
}

fn get_source_url(src: &SourceNode) -> &str {
    match src {
        SourceNode::Archive(a) => &a.src,
        SourceNode::Git(g) => &g.repository,
        _ => "",
    }
}

pub struct ManifestCollection {
    pkg_name: String,
    name: String,
    #[cfg(feature = "libips")]
    manifest: libips::api::Manifest,
}

impl Display for ManifestCollection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

impl ManifestCollection {
    #[cfg(not(feature = "libips"))]
    pub fn new(name: &str) -> Self {
        ManifestCollection {
            pkg_name: name.to_string(),
            name: name.replace("/", "-"),
            #[cfg(feature = "libips")]
            manifest: libips::api::Manifest::default(),
        }
    }
    #[cfg(feature = "libips")]
    pub fn new_with_manifest(name: &str, manifest: libips::api::Manifest) -> Self {
        ManifestCollection {
            pkg_name: name.to_string(),
            name: name.replace("/", "-"),
            manifest,
        }
    }
    #[cfg(feature = "libips")]
    pub fn manifest(&self) -> &libips::api::Manifest {
        &self.manifest
    }
    #[cfg(feature = "libips")]
    pub fn manifest_mut(&mut self) -> &mut libips::api::Manifest {
        &mut self.manifest
    }

    pub fn get_pkg_name(&self) -> String {
        self.pkg_name.clone()
    }

    #[cfg(not(feature = "libips"))]
    pub fn get_base_manifest_name(&self) -> String {
        format!("{}-generated.p5m", self.name)
    }

    #[cfg(not(feature = "libips"))]
    pub fn get_mogrified_name(&self) -> String {
        format!("{}.mogrified.p5m", self.name)
    }

    #[cfg(not(feature = "libips"))]
    pub fn get_depend_name(&self) -> String {
        format!("{}.dep", self.name)
    }

    #[cfg(not(feature = "libips"))]
    pub fn get_resolved_name(&self) -> String {
        format!("{}.dep.res", self.name)
    }

    #[allow(dead_code)]
    pub fn get_final_name(&self) -> String {
        format!("{}.manifest.p5m", self.name)
    }
}

#[cfg(not(feature = "libips"))]
pub fn run_generate_filelist(wks: &Workspace, pkg: &Component) -> Result<()> {
    let proto_path = wks.get_or_create_prototype_dir()?;
    let manifest_path = wks.get_or_create_manifest_dir()?;

    let filelist_path = manifest_path.join("filelist.fmt");
    let formatted_manifest = File::create(&filelist_path)
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "failed to create formatted filelist at {}",
                filelist_path.display()
            )
        })?;

    let pkg_send_cmd = Command::new("pkgsend")
        .arg("generate")
        .arg(proto_path.to_string_lossy().to_string())
        .stdout(Stdio::piped())
        .spawn()
        .into_diagnostic()
        .wrap_err_with(|| {
            let cwd = std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unknown>".into());
            format!(
                "failed to spawn pkgsend generate in cwd {} for prototype dir {}",
                cwd,
                proto_path.display()
            )
        })?;

    let pkg_fmt_cmd_status = Command::new("pkgfmt")
        .stdin(pkg_send_cmd.stdout.unwrap())
        .stdout(formatted_manifest)
        .status()
        .into_diagnostic()
        .wrap_err("failed to run pkgfmt to format filelist from pkgsend output")?;

    if pkg_fmt_cmd_status.success() {
        tracing::info!(target: "pkgdev::ips", "Generated filelist for {}", pkg.get_name());
        Ok(())
    } else {
        Err(miette::miette!("non zero code returned from pkgfmt"))
    }
}

#[cfg(feature = "libips")]
pub fn run_generate_filelist(_wks: &Workspace, _pkg: &Component) -> Result<()> {
    // In libips mode, we do not generate a filelist; manifests are built directly from the prototype.
    Ok(())
}

#[cfg(not(feature = "libips"))]
pub fn generate_manifest_files(
    wks: &Workspace,
    pkg: &Component,
    gate: &Option<Gate>,
    transform_includes: Option<PathBuf>,
) -> Result<Vec<ManifestCollection>> {
    let manifest_path = wks.get_or_create_manifest_dir()?;

    let manifests = if pkg.recipe.package_sections.is_empty() {
        let name = prefixed_name(pkg, pkg.get_name());
        let vars = StringInterpolationVars {
            name: &name,
            version: &pkg.recipe.version.clone().unwrap_or(String::from("0.5.11")), //TODO take this default version from the gate
            build_version: &gate.clone().unwrap_or(Gate::default()).version,
            branch_version: &gate.clone().unwrap_or(Gate::default()).branch,
            revision: &pkg.recipe.revision.clone().unwrap_or(String::from("1")),
            summary: &pkg
                .recipe
                .summary
                .clone()
                .ok_or(miette::miette!("no summary specified"))?,
            classification: &pkg
                .recipe
                .classification
                .clone()
                .ok_or(miette::miette!("no classification specified"))?,
            project_url: &pkg
                .recipe
                .project_url
                .clone()
                .ok_or(miette::miette!("no project_url specified"))?,
            source_url: get_source_url(&pkg.recipe.sources[0].sources[0]),
            license_file_name: &pkg
                .recipe
                .license_file
                .clone()
                .ok_or(miette::miette!("no license_file specified"))?,
            license_name: &pkg
                .recipe
                .license
                .clone()
                .ok_or(miette::miette!("no license specified"))?,
        };
        let mut manifest = render(DEFAULT_IPS_TEMPLATE, vars);

        let drop_dir_line = "\n<transform dir path=.* -> drop>";
        manifest.push_str(drop_dir_line);

        let manifest_collection = ManifestCollection::new(&name);

        let base_path = manifest_path.join(&manifest_collection.get_base_manifest_name());
        write_all(&base_path, &manifest)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to write base manifest {}", base_path.display()))?;
        vec![manifest_collection]
    } else {
        let mut manifests = vec![];
        for p in pkg.recipe.package_sections.iter() {
            let base = p.clone().name.unwrap_or(pkg.get_name());
            let name = prefixed_name(pkg, base);
            let vars = StringInterpolationVars {
                name: &name,
                version: &pkg.recipe.version.clone().unwrap_or(String::from("0.5.11")), //TODO take this default version from the gate
                build_version: &gate.clone().unwrap_or(Gate::default()).version,
                branch_version: &gate.clone().unwrap_or(Gate::default()).branch,
                revision: &pkg.recipe.revision.clone().unwrap_or(String::from("1")),
                summary: &pkg
                    .recipe
                    .summary
                    .clone()
                    .ok_or(miette::miette!("no summary specified"))?,
                classification: &pkg
                    .recipe
                    .classification
                    .clone()
                    .ok_or(miette::miette!("no classification specified"))?,
                project_url: &pkg
                    .recipe
                    .project_url
                    .clone()
                    .ok_or(miette::miette!("no project_url specified"))?,
                source_url: get_source_url(&pkg.recipe.sources[0].sources[0]),
                license_file_name: &pkg
                    .recipe
                    .license_file
                    .clone()
                    .ok_or(miette::miette!("no license_file specified"))?,
                license_name: &pkg
                    .recipe
                    .license
                    .clone()
                    .ok_or(miette::miette!("no license specified"))?,
            };
            let mut manifest = render(DEFAULT_IPS_TEMPLATE, vars);
            let default_action_keep_line =
                "\n<transform file link hardlink path=.* -> default keep false>";
            manifest.push_str(default_action_keep_line);

            generate_transform_lines(&mut manifest, &p.files);
            generate_transform_lines(&mut manifest, &p.links);
            generate_transform_lines(&mut manifest, &p.hardlinks);
            let drop_actions_line = "\n<transform file link hardlink keep=false -> drop>";
            manifest.push_str(drop_actions_line);

            let cleanup_line = "\n<transform file link hardlink keep=true -> delete keep true>";
            manifest.push_str(cleanup_line);

            let drop_dir_line = "\n<transform dir path=.* -> drop>";
            manifest.push_str(drop_dir_line);

            let manifest_collection = ManifestCollection::new(&name);
            let base_path = manifest_path.join(manifest_collection.get_base_manifest_name());
            write_all(&base_path, &manifest)
                .into_diagnostic()
                .wrap_err_with(|| {
                    format!("failed to write base manifest {}", base_path.display())
                })?;
            manifests.push(manifest_collection);
        }
        manifests
    };

    let include_path = if let Some(gate) = gate {
        if !gate.default_transforms.is_empty() {
            let mut include_str = gate
                .default_transforms
                .clone()
                .into_iter()
                .map(|tr| tr.to_transform_line())
                .collect::<Vec<String>>()
                .join("\n");
            include_str.push_str("\n");
            let inc_path = manifest_path.join("includes.mog");
            tracing::info!(target: "pkgdev::ips", "Adding includes {} to includes.mog", &include_str);
            write_all(&inc_path, &include_str)
                .into_diagnostic()
                .wrap_err_with(|| {
                    format!("failed to write includes.mog at {}", inc_path.display())
                })?;
            Some(inc_path.to_string_lossy().to_string())
        } else {
            tracing::info!(target: "pkgdev::ips", "Gate {} has no transforms", gate.name);
            None
        }
    } else {
        println!("Not building against a gate not adding gate transforms");
        None
    };

    for manifest in manifests.iter() {
        let mog_path = manifest_path.join(manifest.get_mogrified_name());
        let mogrified_manifest = File::create(&mog_path)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "failed to create mogrified manifest at {}",
                    mog_path.display()
                )
            })?;
        let mut pkg_mogrify_cmd = Command::new("pkgmogrify");

        if let Some(includes_path) = transform_includes.clone() {
            pkg_mogrify_cmd.arg("-I").arg(&includes_path);
        }
        pkg_mogrify_cmd
            .current_dir("..")
            .arg(
                manifest_path
                    .join(manifest.get_base_manifest_name())
                    .to_string_lossy()
                    .to_string(),
            )
            .arg(
                manifest_path
                    .join("filelist.fmt")
                    .to_string_lossy()
                    .to_string(),
            );

        if let Some(includes) = include_path.clone() {
            pkg_mogrify_cmd.arg(&includes);
        }

        if let Some(mog_file_path) = pkg.get_mogrify_manifest() {
            pkg_mogrify_cmd.arg(&mog_file_path.to_string_lossy().to_string());
        }

        pkg_mogrify_cmd.stdout(Stdio::piped());
        let pkg_mogrify_status = pkg_mogrify_cmd
            .spawn()
            .into_diagnostic()
            .wrap_err_with(|| {
                let cwd = std::env::current_dir()
                    .ok()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<unknown>".into());
                format!(
                    "failed to spawn pkgmogrify in cwd {} for base manifest '{}'; includes: {:?}",
                    cwd,
                    manifest.get_base_manifest_name(),
                    include_path
                )
            })?;

        let pkg_fmt_cmd_status = Command::new("pkgfmt")
            .stdin(pkg_mogrify_status.stdout.unwrap())
            .stdout(mogrified_manifest)
            .status()
            .into_diagnostic()
            .wrap_err_with(|| "failed to run pkgfmt to format mogrified manifest".to_string())?;

        if pkg_fmt_cmd_status.success() {
            tracing::info!(target: "pkgdev::ips", "Finished manifest transformations for manifest {}", &manifest);
        } else {
            return Err(miette::miette!("non zero code returned from pkgfmt"));
        }
    }

    Ok(manifests)
}

#[cfg(feature = "libips")]
pub fn generate_manifest_files(
    _wks: &Workspace,
    pkg: &Component,
    gate: &Option<Gate>,
    _transform_includes: Option<PathBuf>,
) -> Result<Vec<ManifestCollection>> {
    let mut collections: Vec<ManifestCollection> = Vec::new();

    let build_version = gate.clone().unwrap_or_default().version;
    let branch_version = gate.clone().unwrap_or_default().branch;

    let build_for_name = |pkg_name: String| -> Result<ManifestCollection> {
        // Prefer fmri from metadata, falling back to provided name
        let pkg_name = prefixed_name(pkg, pkg_name);

        // Compose FMRI components
        let version = pkg
            .recipe
            .version
            .clone()
            .unwrap_or_else(|| "0.5.11".to_string());
        let revision = pkg
            .recipe
            .revision
            .clone()
            .unwrap_or_else(|| "1".to_string());
        let publisher = gate.clone().unwrap_or_default().publisher;
        let fmri_str = format!(
            "pkg://{publisher}/{pkg_name}@{version},{build_version}-{branch_version}.{revision}"
        );
        let fmri = libips::api::Fmri::parse(&fmri_str)
            .into_diagnostic()
            .wrap_err("failed to parse FMRI for manifest")?;
        let summary = pkg
            .recipe
            .summary
            .clone()
            .ok_or_else(|| miette::miette!("no summary specified"))?;
        let classification = pkg
            .recipe
            .classification
            .clone()
            .ok_or_else(|| miette::miette!("no classification specified"))?;
        let project_url = pkg
            .recipe
            .project_url
            .clone()
            .ok_or_else(|| miette::miette!("no project_url specified"))?;
        // Derive a source_url; fall back to project_url if no sources are present
        let source_url = if let Some(sec) = pkg.recipe.sources.get(0) {
            if let Some(src) = sec.sources.get(0) {
                get_source_url(src).to_string()
            } else {
                project_url.clone()
            }
        } else {
            project_url.clone()
        };
        // Determine license file: prefer recipe.license_file else common filenames in component dir; if not found, skip
        let license_file_name: Option<String> = if let Some(f) = pkg.recipe.license_file.clone() {
            Some(f)
        } else {
            let base = pkg.get_path();
            let candidates = [
                "LICENSE",
                "LICENSE.md",
                "COPYING",
                "COPYRIGHT",
                "LICENCE",
                "LICENCE.md",
            ];
            candidates
                .iter()
                .map(|c| base.join(c))
                .find(|p| p.exists())
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
        };
        let license_name: Option<String> = pkg.recipe.license.clone();
        if license_file_name.is_none() || license_name.is_none() {
            tracing::warn!(target: "pkgdev::ips", "No license file/name found for {}; proceeding without license action", pkg.get_name());
        }

        // Build typed manifest via libips using plausible builder methods
        let mut builder = libips::api::ManifestBuilder::new();
        builder.add_set("pkg.fmri", &fmri);
        builder.add_set("pkg.summary", &summary);
        builder.add_set(
            "info.classification",
            &format!("org.opensolaris.category.2008:{classification}"),
        );
        builder.add_set("info.upstream-url", &project_url);
        builder.add_set("info.source-url", &source_url);
        if let (Some(ref lf), Some(ref ln)) = (license_file_name.as_ref(), license_name.as_ref()) {
            builder.add_license(lf.as_str(), ln.as_str());
        } else {
            tracing::warn!(target: "pkgdev::ips", "Skipping license action for {} (missing file and/or name)", pkg.get_name());
        }
        let manifest = builder.build();

        Ok(ManifestCollection::new_with_manifest(&pkg_name, manifest))
    };

    if pkg.recipe.package_sections.is_empty() {
        let name = pkg.get_name();
        let collection = build_for_name(name)?;
        collections.push(collection);
    } else {
        for p in pkg.recipe.package_sections.iter() {
            let name = p.clone().name.unwrap_or(pkg.get_name());
            let collection = build_for_name(name)?;
            collections.push(collection);
        }
    }

    Ok(collections)
}

#[allow(dead_code)]
fn generate_transform_lines(manifest: &mut String, nodes: &Vec<TransformNode>) {
    for node in nodes {
        for (attribute, selector) in node.selectors.iter() {
            let tranforms_string = format!(
                "\n<transform {} {}={} -> set keep true>",
                &node.action, &attribute, &selector
            );
            manifest.push_str(&tranforms_string);
        }
    }
}

#[cfg(not(feature = "libips"))]
pub fn run_generate_pkgdepend(
    wks: &Workspace,
    manifests: &mut [ManifestCollection],
    _repo_base: &std::path::Path,
) -> Result<()> {
    let manifest_path = wks.get_or_create_manifest_dir()?;
    let prototype_path = wks.get_or_create_prototype_dir()?;

    for manifest in manifests {
        let dep_path = manifest_path.join(manifest.get_depend_name());
        let depend_manifest = File::create(&dep_path)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "failed to create dependency manifest {}",
                    dep_path.display()
                )
            })?;

        let pkg_depend_cmd = Command::new("pkgdepend")
            .arg("generate")
            .arg("-m")
            .arg("-d")
            .arg(prototype_path.to_string_lossy().to_string())
            .arg(
                manifest_path
                    .join(manifest.get_mogrified_name())
                    .to_string_lossy()
                    .to_string(),
            )
            .stdout(Stdio::piped())
            .spawn()
            .into_diagnostic()
            .wrap_err_with(|| {
                let cwd = std::env::current_dir()
                    .ok()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<unknown>".into());
                format!(
                    "failed to spawn pkgdepend generate in cwd {} using prototype {} and mogrified {}",
                    cwd,
                    prototype_path.display(),
                    manifest_path.join(manifest.get_mogrified_name()).display()
                )
            })?;

        let pkg_fmt_cmd_status = Command::new("pkgfmt")
            .stdin(pkg_depend_cmd.stdout.unwrap())
            .stdout(depend_manifest)
            .status()
            .into_diagnostic()
            .wrap_err("failed to run pkgfmt to format dependency manifest")?;

        if pkg_fmt_cmd_status.success() {
            println!("Generated dependency entries for manifest {}", manifest);
        } else {
            return Err(miette::miette!(
                "dependency generation failed for manifest {}",
                manifest
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "libips")]
pub fn run_generate_pkgdepend(
    wks: &Workspace,
    manifests: &mut [ManifestCollection],
    repo_base: &std::path::Path,
) -> Result<()> {
    use libips::repository::{FileBackend, ReadableRepository};
    use std::path::Path;
    // Prototype directory is required for file-level dependency generation
    let proto_dir = wks.get_or_create_prototype_dir()?;
    // Open repository backend to allow resolution of deps to FMRIs
    let mut backend = FileBackend::open(repo_base)
        .into_diagnostic()
        .wrap_err("failed to open IPS repository backend for dependency generation")?;

    for m in manifests.iter_mut() {
        let updated = libips::api::DependencyGenerator::generate_with_repo(
            &mut backend,
            None, // resolve across all publishers in the repo
            Path::new(&proto_dir),
            m.manifest(),
            libips::api::DependGenerateOptions::default(),
        )
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to generate dependencies for manifest {}", m))?;

        // replace manifest with the dependency-enriched one
        *m.manifest_mut() = updated;
    }

    Ok(())
}

#[cfg(not(feature = "libips"))]
pub fn run_resolve_dependencies(
    wks: &Workspace,
    manifests: &mut [ManifestCollection],
) -> Result<()> {
    let manifest_path = wks.get_or_create_manifest_dir()?;

    println!("Attempting to resolve runtime dependencies");
    let pkg_depend_cmd = Command::new("pkgdepend")
        .arg("resolve")
        .arg("-m")
        .arg("-v")
        .args(
            manifests
                .iter()
                .map(|manifest| {
                    manifest_path
                        .join(manifest.get_depend_name())
                        .to_string_lossy()
                        .to_string()
                })
                .collect::<Vec<_>>(),
        )
        .stdout(Stdio::inherit())
        .status()
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "failed to run pkgdepend resolve for manifests in {}",
                manifest_path.display()
            )
        })?;

    if pkg_depend_cmd.success() {
        println!("Resolved dependencies");
    } else {
        return Err(miette::miette!("failed to resolve dependencies",));
    }
    Ok(())
}

#[cfg(feature = "libips")]
pub fn run_resolve_dependencies(
    _wks: &Workspace,
    _manifests: &mut [ManifestCollection],
) -> Result<()> {
    Ok(())
}

#[cfg(not(feature = "libips"))]
pub fn run_lint(wks: &Workspace, manifests: &[ManifestCollection]) -> Result<()> {
    let manifest_path = wks.get_or_create_manifest_dir()?;

    for manifest in manifests {
        let res_path = manifest_path.join(manifest.get_resolved_name());
        let pkg_lint_cmd = Command::new("pkglint")
            .arg(res_path.to_string_lossy().to_string())
            .stdout(Stdio::inherit())
            .status()
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to run pkglint for {}", res_path.display()))?;

        if pkg_lint_cmd.success() {
            println!("Lint success for manifest {}", manifest);
        } else {
            return Err(miette::miette!("Lint failed for manifest {}", manifest));
        }
    }
    Ok(())
}

#[cfg(feature = "libips")]
pub fn run_lint(_wks: &Workspace, manifests: &[ManifestCollection]) -> Result<()> {
    for m in manifests.iter() {
        // Use libips lint facade; if the API requires a config, pass Default.
        libips::api::lint::lint_manifest(m.manifest(), &Default::default())
            .into_diagnostic()
            .wrap_err_with(|| format!("manifest lint failed for {}", m))?;
    }
    Ok(())
}

#[cfg(not(feature = "libips"))]
pub fn ensure_repo_with_publisher_exists(
    repo_base: &std::path::Path,
    publisher: &str,
) -> Result<()> {
    if !repo_base.join("pkg5.repository").exists() {
        let pkg_repo_status = Command::new("pkgrepo")
            .arg("create")
            .arg(&repo_base.to_string_lossy().to_string())
            .stdout(Stdio::inherit())
            .status()
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "failed to run pkgrepo create for repo {}",
                    repo_base.display()
                )
            })?;
        if !pkg_repo_status.success() {
            return Err(miette::miette!(
                "pkgrepo create failed with non zero exit code"
            ));
        }
    }

    if !repo_base.join("publisher").join(publisher).exists() {
        let pkg_repo_status = Command::new("pkgrepo")
            .arg("add-publisher")
            .arg("-s")
            .arg(&repo_base.to_string_lossy().to_string())
            .arg(publisher)
            .stdout(Stdio::inherit())
            .status()
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "failed to run pkgrepo add-publisher '{}' for repo {}",
                    publisher,
                    repo_base.display()
                )
            })?;
        if !pkg_repo_status.success() {
            return Err(miette::miette!(
                "pkgrepo create failed with non zero exit code"
            ));
        }
    }

    Ok(())
}

#[cfg(feature = "libips")]
pub fn ensure_repo_with_publisher_exists(
    repo_base: &std::path::Path,
    publisher: &str,
) -> Result<()> {
    // Prefer open, create if missing
    let repo = if repo_base.join("pkg5.repository").exists() {
        libips::api::Repository::open(repo_base)
            .into_diagnostic()
            .wrap_err("failed to open IPS repository")?
    } else {
        libips::api::Repository::create(repo_base)
            .into_diagnostic()
            .wrap_err("failed to create IPS repository")?
    };

    let has_pub = repo
        .has_publisher(publisher)
        .into_diagnostic()
        .wrap_err("failed to query publisher in IPS repository")?;
    if !has_pub {
        repo.add_publisher(publisher)
            .into_diagnostic()
            .wrap_err("failed to add publisher to IPS repository")?;
    }
    Ok(())
}

#[cfg(not(feature = "libips"))]
pub fn publish(
    wks: &Workspace,
    pkg: &Component,
    publisher: &str,
    manifests: &[ManifestCollection],
    repo_base: &std::path::Path,
) -> Result<()> {
    let proto_dir = wks.get_or_create_prototype_dir()?;
    let build_dir = wks.get_or_create_build_dir()?;
    let unpack_name = derive_source_name(pkg.recipe.name.clone());
    let unpack_path = build_dir.join(&unpack_name);

    for manifest in manifests {
        let manifest_path = wks
            .get_or_create_manifest_dir()?
            .join(manifest.get_resolved_name());

        let pkgsend_status = Command::new("pkgsend")
            .arg("publish")
            .arg("-d")
            .arg(&proto_dir.to_string_lossy().to_string())
            .arg("-d")
            .arg(&unpack_path.to_string_lossy().to_string())
            .arg("-d")
            .arg(&pkg.get_path())
            .arg("-s")
            .arg(&repo_base.to_string_lossy().to_string())
            .arg(&manifest_path.to_string_lossy().to_string())
            .stdout(Stdio::inherit())
            .status()
            .into_diagnostic()
            .wrap_err_with(|| format!(
                "failed to run pkgsend publish to repo {} for manifest {} (proto {}, unpack {}, pkgdir {})",
                repo_base.display(),
                manifest_path.display(),
                proto_dir.display(),
                unpack_path.display(),
                pkg.get_path().display()
            ))?;

        if pkgsend_status.success() {
            println!("Published manifest {}", manifest);
            println!(
                "Install with pkg set-publisher {}; pkg install -g {} {}",
                publisher,
                repo_base.display(),
                manifest.get_pkg_name()
            );
        } else {
            return Err(miette::miette!("publish failed for {}", manifest));
        }
    }
    Ok(())
}

#[cfg(feature = "libips")]
pub fn publish(
    wks: &Workspace,
    _pkg: &Component,
    publisher: &str,
    manifests: &[ManifestCollection],
    repo_base: &std::path::Path,
) -> Result<()> {
    let proto_dir = wks.get_or_create_prototype_dir()?;
    // Open or create repository
    let repo = if repo_base.join("pkg5.repository").exists() {
        libips::api::Repository::open(repo_base)
            .into_diagnostic()
            .wrap_err("failed to open IPS repository")?
    } else {
        libips::api::Repository::create(repo_base)
            .into_diagnostic()
            .wrap_err("failed to create IPS repository")?
    };

    let client = libips::api::PublisherClient::new(repo, publisher.to_string());

    for m in manifests {
        let mut txn = client
            .begin()
            .into_diagnostic()
            .wrap_err("failed to begin IPS txn")?;
        // Only include the staged prototype directory as payload to ensure we publish
        // just what was staged during the build, not the entire source tree.
        txn.add_payload_dir(&proto_dir)
            .into_diagnostic()
            .wrap_err("failed to add prototype dir to txn")?;

        // Add the typed manifest
        txn.add_manifest(
            #[cfg(feature = "libips")]
            m.manifest(),
        );

        txn.commit()
            .into_diagnostic()
            .wrap_err("failed to commit IPS transaction")?;

        println!(
            "Published manifest {}. Install with: pkg set-publisher {}; pkg install -g {} {}",
            m,
            publisher,
            repo_base.display(),
            m.get_pkg_name()
        );
    }

    Ok(())
}
