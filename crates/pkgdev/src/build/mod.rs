mod automake;
mod compile;
mod dependencies;
mod install;
pub mod ips;
mod script;
mod tarball;
mod util;

use clap::{Parser, ValueEnum};
use workspace::Workspace;

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuildSteps {
    Download,
    Unpack,
    Build,
}

#[derive(Debug, Parser)]
pub struct BuildArgs {
    #[arg(long = "step", short)]
    pub stop_on_step: Option<BuildSteps>,

    #[arg(long, default_value = "false")]
    pub no_clean: bool,

    #[arg(long, default_value = "false")]
    pub archive_clean: bool,

    #[arg(short = 'I', long = "include")]
    pub transform_include_dir: Option<PathBuf>,
}

use std::path::PathBuf;

use crate::build::dependencies::ensure_packages_are_installed;
use crate::sources::{download_sources, unpack};
use automake::build_using_automake;
use component::Component;
use component::SourceSection;
use forge_config::Settings;
use gate::Gate;
use miette::{IntoDiagnostic, Result, WrapErr};
use script::build_using_scripts;

pub fn build_package_sources(wks: &Workspace, pkg: &Component, settings: &Settings) -> Result<()> {
    for section in pkg.recipe.build_sections.iter() {
        if let Some(c) = section.configure.clone() {
            build_using_automake(wks, pkg, &c, settings)?;
        } else if let Some(_) = section.cmake {
            unimplemented!();
        } else if let Some(_) = section.meson {
            unimplemented!();
        } else if let Some(script) = section.script.clone() {
            build_using_scripts(wks, pkg, &script, settings)?;
        }
    }

    Ok(())
}

pub async fn run_build(
    component: &Component,
    gate: &Option<Gate>,
    wks: &Workspace,
    settings: &Settings,
    args: &BuildArgs,
    repo_mgr: &crate::repo::RepoManager,
    repo_override_path: Option<PathBuf>,
    repo_override_context: Option<String>,
) -> Result<()> {
    let transform_include_dir =
        args.transform_include_dir
            .clone()
            .map(|p| match p.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    println!(
                        "could not canonicalize {} due to {} continuing ignoring and continuing",
                        p.display(),
                        e
                    );
                    p
                }
            });

    if !args.no_clean {
        std::fs::remove_dir_all(wks.get_or_create_download_dir()?)
            .into_diagnostic()
            .wrap_err(format!(
                "could not clean the download directory in workspace {0}",
                wks.get_root_path().display()
            ))?;
        std::fs::remove_dir_all(wks.get_or_create_build_dir()?)
            .into_diagnostic()
            .wrap_err(format!(
                "could not clean the build directory in workspace {0}",
                wks.get_root_path().display()
            ))?;
        std::fs::remove_dir_all(wks.get_or_create_prototype_dir()?)
            .into_diagnostic()
            .wrap_err(format!(
                "could not clean the prototype directory in workspace {0}",
                wks.get_root_path().display()
            ))?;
        std::fs::remove_dir_all(wks.get_or_create_manifest_dir()?)
            .into_diagnostic()
            .wrap_err(format!(
                "could not clean the manifest directory in workspace {0}",
                wks.get_root_path().display()
            ))?;
    }

    ensure_packages_are_installed(wks, false, &component)?;

    let sources: Vec<SourceSection> = component.recipe.sources.clone();

    download_sources(component, wks, args.archive_clean)
        .await
        .wrap_err("download and verify failed")?;

    if let Some(stop_on_step) = &args.stop_on_step {
        if stop_on_step == &BuildSteps::Download {
            return Ok(());
        }
    }

    unpack::unpack_sources(&component, &wks, sources.as_slice()).wrap_err("unpack step failed")?;

    if let Some(stop_on_step) = &args.stop_on_step {
        if stop_on_step == &BuildSteps::Unpack {
            return Ok(());
        }
    }

    build_package_sources(&wks, &component, &settings).wrap_err("configure step failed")?;

    if let Some(stop_on_step) = &args.stop_on_step {
        if stop_on_step == &BuildSteps::Build {
            return Ok(());
        }
    }

    let distribution_type = gate
        .clone()
        .unwrap_or_default()
        .distribution
        .clone()
        .unwrap_or_default()
        .distribution_type
        .clone();

    match distribution_type {
        gate::DistributionType::Tarbball => {
            tarball::make_release_tarball(&wks, &component)?;
        }
        gate::DistributionType::IPS => {
            // Resolve repository path using the manager and CLI overrides
            let repo_path =
                repo_mgr.resolve(repo_override_path.clone(), repo_override_context.clone())?;
            run_ips_actions(
                &wks,
                &component,
                gate,
                transform_include_dir,
                repo_path.as_path(),
            )?;
        }
    }

    Ok(())
}

fn run_ips_actions(
    wks: &Workspace,
    pkg: &Component,
    gate: &Option<Gate>,
    transform_include_dir: Option<PathBuf>,
    repo_path: &std::path::Path,
) -> Result<()> {
    ips::run_generate_filelist(wks, pkg).wrap_err("generating file list failed")?;

    let mut manifests = ips::generate_manifest_files(wks, pkg, gate, transform_include_dir)
        .wrap_err("mogrify failed")?;

    ips::run_generate_pkgdepend(wks, &mut manifests, repo_path)
        .wrap_err("failed to generate dependency entries")?;

    ips::run_resolve_dependencies(wks, &mut manifests)
        .wrap_err("failed to resolve dependencies")?;

    ips::run_lint(wks, manifests.as_slice()).wrap_err("lint failed")?;

    let publisher = gate.clone().unwrap_or_default().publisher;
    ips::ensure_repo_with_publisher_exists(repo_path, &publisher)
        .wrap_err("failed to ensure repository exists")?;

    ips::publish(wks, pkg, &publisher, manifests.as_slice(), repo_path)
        .wrap_err("package publish failed")?;

    Ok(())
}
