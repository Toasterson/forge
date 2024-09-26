use crate::sources::derive_source_name;
use component::{Component, SourceNode, TransformNode};
use config::Settings;
use fs_extra::file::write_all;
use gate::Gate;
use microtemplate::{render, Substitutions};
use miette::{IntoDiagnostic, Result};
use std::fmt::{Display, Formatter};
use std::{
    fs::File,
    path::PathBuf,
    process::{Command, Stdio},
};
use workspace::Workspace;

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

fn get_source_url<'a>(src: &'a SourceNode) -> &'a str {
    match src {
        SourceNode::Archive(a) => &a.src,
        SourceNode::Git(g) => &g.repository,
        _ => "",
    }
}

pub struct ManifestCollection {
    pkg_name: String,
    name: String,
}

impl Display for ManifestCollection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

impl ManifestCollection {
    pub fn new(name: &str) -> Self {
        ManifestCollection {
            pkg_name: name.to_string(),
            name: name.replace("/", "-"),
        }
    }

    pub fn get_pkg_name(&self) -> String {
        self.pkg_name.clone()
    }

    pub fn get_base_manifest_name(&self) -> String {
        format!("{}-generated.p5m", self.name)
    }

    pub fn get_mogrified_name(&self) -> String {
        format!("{}.mogrified.p5m", self.name)
    }

    pub fn get_depend_name(&self) -> String {
        format!("{}.dep", self.name)
    }

    pub fn get_resolved_name(&self) -> String {
        format!("{}.dep.res", self.name)
    }

    pub fn get_final_name(&self) -> String {
        format!("{}.manifest.p5m", self.name)
    }
}

pub fn run_generate_filelist(wks: &Workspace, pkg: &Component) -> Result<()> {
    let proto_path = wks.get_or_create_prototype_dir()?;
    let manifest_path = wks.get_or_create_manifest_dir()?;

    let formatted_manifest = File::create(manifest_path.join("filelist.fmt")).into_diagnostic()?;

    let pkg_send_cmd = Command::new("pkgsend")
        .arg("generate")
        .arg(proto_path.to_string_lossy().to_string())
        .stdout(Stdio::piped())
        .spawn()
        .into_diagnostic()?;

    let pkg_fmt_cmd_status = Command::new("pkgfmt")
        .stdin(pkg_send_cmd.stdout.unwrap())
        .stdout(formatted_manifest)
        .status()
        .into_diagnostic()?;

    if pkg_fmt_cmd_status.success() {
        println!("Generated filelist for {}", pkg.get_name());
        Ok(())
    } else {
        Err(miette::miette!("non zero code returned from pkgfmt"))
    }
}

pub fn generate_manifest_files(
    wks: &Workspace,
    pkg: &Component,
    gate: &Option<Gate>,
    transform_includes: Option<PathBuf>,
) -> Result<Vec<ManifestCollection>> {
    let manifest_path = wks.get_or_create_manifest_dir()?;

    let manifests = if pkg.recipe.package_sections.is_empty() {
        let name = pkg.get_name();
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

        let manifest_collection = ManifestCollection::new(&pkg.get_name());

        write_all(
            manifest_path.join(&manifest_collection.get_base_manifest_name()),
            &manifest,
        )
        .into_diagnostic()?;
        vec![manifest_collection]
    } else {
        let mut manifests = vec![];
        for p in pkg.recipe.package_sections.iter() {
            let name = p.clone().name.unwrap_or(pkg.get_name());
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
            let default_action_keep_line = "\n<transform file link hardlink path=.* -> default keep false>";
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
            write_all(
                manifest_path.join(manifest_collection.get_base_manifest_name()),
                &manifest,
            )
            .into_diagnostic()?;
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
                .map(|tr| tr.to_string())
                .collect::<Vec<String>>()
                .join("\n");
            include_str.push_str("\n");
            let inc_path = manifest_path.join("includes.mog");
            println!("Adding includes {} to includes.mog", &include_str);
            write_all(&inc_path, &include_str).into_diagnostic()?;
            Some(inc_path.to_string_lossy().to_string())
        } else {
            println!("Gate {} has no transforms", gate.name);
            None
        }
    } else {
        println!("Not building against a gate not adding gate transforms");
        None
    };

    for manifest in manifests.iter() {
        let mogrified_manifest =
            File::create(manifest_path.join(manifest.get_mogrified_name())).into_diagnostic()?;
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
        let pkg_mogrify_status = pkg_mogrify_cmd.spawn().into_diagnostic()?;

        let pkg_fmt_cmd_status = Command::new("pkgfmt")
            .stdin(pkg_mogrify_status.stdout.unwrap())
            .stdout(mogrified_manifest)
            .status()
            .into_diagnostic()?;

        if pkg_fmt_cmd_status.success() {
            println!(
                "Finished manifest transformations for manifest {}",
                &manifest
            );
        } else {
            return Err(miette::miette!("non zero code returned from pkgfmt"));
        }
    }

    Ok(manifests)
}

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

pub fn run_generate_pkgdepend(wks: &Workspace, manifests: &[ManifestCollection]) -> Result<()> {
    let manifest_path = wks.get_or_create_manifest_dir()?;
    let prototype_path = wks.get_or_create_prototype_dir()?;

    for manifest in manifests {
        let depend_manifest =
            File::create(manifest_path.join(manifest.get_depend_name())).into_diagnostic()?;

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
            .into_diagnostic()?;

        let pkg_fmt_cmd_status = Command::new("pkgfmt")
            .stdin(pkg_depend_cmd.stdout.unwrap())
            .stdout(depend_manifest)
            .status()
            .into_diagnostic()?;

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

pub fn run_resolve_dependencies(wks: &Workspace, manifests: &[ManifestCollection]) -> Result<()> {
    let manifest_path = wks.get_or_create_manifest_dir()?;

    println!("Attempting to resolve runtime dependencies");
    let pkg_depend_cmd = Command::new("pkgdepend")
        .arg("resolve")
        .arg("-v")
        .args(manifests.iter().map(|manifest| manifest_path
            .join(manifest.get_depend_name())
            .to_string_lossy()
            .to_string()).collect::<Vec<_>>())
        .stdout(Stdio::inherit())
        .status()
        .into_diagnostic()?;

    if pkg_depend_cmd.success() {
        println!("Resolved dependencies");
    } else {
        return Err(miette::miette!(
                "failed to resolve dependencies",
            ));
    }
    Ok(())
}

pub fn build_final_manifest(wks: &Workspace, manifests: &[ManifestCollection]) -> Result<()> {
    let manifest_path = wks.get_or_create_manifest_dir()?;

    for manifest in manifests {
        let pkg_mogrify_cmd = Command::new("pkgmogrify")
            .arg("-O")
            .arg(
                manifest_path
                    .join(manifest.get_final_name())
                    .to_string_lossy()
                    .to_string(),
            )
            .arg(
                manifest_path
                    .join(manifest.get_mogrified_name())
                    .to_string_lossy()
                    .to_string(),
            )
            .arg(
                manifest_path
                    .join(manifest.get_resolved_name())
                    .to_string_lossy()
                    .to_string(),
            )
            .status()
            .into_diagnostic()?;

        if pkg_mogrify_cmd.success() {
            println!("Final Manifest for {} created", manifest);
        } else {
            return Err(miette::miette!(
                "Final Manifest creation for {} failed",
                manifest
            ));
        }
    }
    Ok(())
}

pub fn run_lint(wks: &Workspace, manifests: &[ManifestCollection]) -> Result<()> {
    let manifest_path = wks.get_or_create_manifest_dir()?;

    for manifest in manifests {
        let pkg_lint_cmd = Command::new("pkglint")
            .arg(
                manifest_path
                    .join(manifest.get_final_name())
                    .to_string_lossy()
                    .to_string(),
            )
            .stdout(Stdio::inherit())
            .status()
            .into_diagnostic()?;

        if pkg_lint_cmd.success() {
            println!("Lint success for manifest {}", manifest);
        } else {
            return Err(miette::miette!("Lint failed for manifest {}", manifest));
        }
    }
    Ok(())
}

pub fn ensure_repo_with_publisher_exists(publisher: &str) -> Result<()> {
    let repo_base = Settings::get_or_create_repo_dir().into_diagnostic()?;

    if !repo_base.join("pkg5.repository").exists() {
        let pkg_repo_status = Command::new("pkgrepo")
            .arg("create")
            .arg(&repo_base.to_string_lossy().to_string())
            .stdout(Stdio::inherit())
            .status()
            .into_diagnostic()?;
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
            .into_diagnostic()?;
        if !pkg_repo_status.success() {
            return Err(miette::miette!(
                "pkgrepo create failed with non zero exit code"
            ));
        }
    }

    Ok(())
}

pub fn publish(
    wks: &Workspace,
    pkg: &Component,
    publisher: &str,
    manifests: &[ManifestCollection],
) -> Result<()> {
    let proto_dir = wks.get_or_create_prototype_dir()?;
    let build_dir = wks.get_or_create_build_dir()?;
    let unpack_name = derive_source_name(pkg.recipe.name.clone());
    let unpack_path = build_dir.join(&unpack_name);
    let repo_path = Settings::get_or_create_repo_dir().into_diagnostic()?;

    for manifest in manifests {
        let manifest_path = wks
            .get_or_create_manifest_dir()?
            .join(manifest.get_final_name());

        let pkgsend_status = Command::new("pkgsend")
            .arg("publish")
            .arg("-d")
            .arg(&proto_dir.to_string_lossy().to_string())
            .arg("-d")
            .arg(&unpack_path.to_string_lossy().to_string())
            .arg("-d")
            .arg(&pkg.get_path())
            .arg("-s")
            .arg(&repo_path.to_string_lossy().to_string())
            .arg(&manifest_path.to_string_lossy().to_string())
            .stdout(Stdio::inherit())
            .status()
            .into_diagnostic()?;

        if pkgsend_status.success() {
            println!("Published manifest {}", manifest);
            println!(
                "Install with pkg set-publisher {}; pkg install -g {} {}",
                publisher,
                repo_path.display(),
                manifest.get_pkg_name()
            );
        } else {
            return Err(miette::miette!("publish failed for {}", manifest));
        }
    }
    Ok(())
}
