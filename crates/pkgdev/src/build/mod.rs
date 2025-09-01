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
    tracing::info!(target: "pkgdev::build", "[build] Starting build for component: {}", component.get_name());
    let transform_include_dir =
        args.transform_include_dir
            .clone()
            .map(|p| match p.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(target: "pkgdev::build", "[build] could not canonicalize {} due to {} continuing ignoring and continuing", p.display(), e);
                    p
                }
            });

    if !args.no_clean {
        tracing::info!(target: "pkgdev::build", "[build] Cleaning workspace directories at {}", wks.get_root_path().display());
        let dl = wks.get_or_create_download_dir()?;
        if let Err(e) = std::fs::remove_dir_all(&dl) {
            if e.kind() != std::io::ErrorKind::NotFound {
                Err(e).into_diagnostic().wrap_err(format!(
                    "could not clean the download directory in workspace {0}",
                    wks.get_root_path().display()
                ))?;
            }
        }
        let bld = wks.get_or_create_build_dir()?;
        if let Err(e) = std::fs::remove_dir_all(&bld) {
            if e.kind() != std::io::ErrorKind::NotFound {
                Err(e).into_diagnostic().wrap_err(format!(
                    "could not clean the build directory in workspace {0}",
                    wks.get_root_path().display()
                ))?;
            }
        }
        let proto = wks.get_or_create_prototype_dir()?;
        if let Err(e) = std::fs::remove_dir_all(&proto) {
            if e.kind() != std::io::ErrorKind::NotFound {
                Err(e).into_diagnostic().wrap_err(format!(
                    "could not clean the prototype directory in workspace {0}",
                    wks.get_root_path().display()
                ))?;
            }
        }
        let mani = wks.get_or_create_manifest_dir()?;
        if let Err(e) = std::fs::remove_dir_all(&mani) {
            if e.kind() != std::io::ErrorKind::NotFound {
                Err(e).into_diagnostic().wrap_err(format!(
                    "could not clean the manifest directory in workspace {0}",
                    wks.get_root_path().display()
                ))?;
            }
        }
    } else {
        tracing::info!(target: "pkgdev::build", "[build] Skipping clean (no_clean=true)");
    }

    ensure_packages_are_installed(wks, false, &component)?;

    tracing::info!(target: "pkgdev::build", "[build] Starting download step (archive_clean: {})", args.archive_clean);
    let sources: Vec<SourceSection> = component.recipe.sources.clone();

    download_sources(component, wks, args.archive_clean)
        .await
        .wrap_err("download and verify failed")?;
    tracing::info!(target: "pkgdev::build", "[build] Download completed");

    if let Some(stop_on_step) = &args.stop_on_step {
        if stop_on_step == &BuildSteps::Download {
            return Ok(());
        }
    }

    tracing::info!(target: "pkgdev::build", "[build] Starting unpack step");
    unpack::unpack_sources(&component, &wks, sources.as_slice()).wrap_err("unpack step failed")?;
    tracing::info!(target: "pkgdev::build", "[build] Unpack completed");

    if let Some(stop_on_step) = &args.stop_on_step {
        if stop_on_step == &BuildSteps::Unpack {
            return Ok(());
        }
    }

    tracing::info!(target: "pkgdev::build", "[build] Starting build/compile step");
    build_package_sources(&wks, &component, &settings).wrap_err("configure step failed")?;
    tracing::info!(target: "pkgdev::build", "[build] Build/compile completed");

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

    let dist_str = match &distribution_type {
        gate::DistributionType::Tarbball => "tarball",
        gate::DistributionType::IPS => "ips",
    };
    tracing::info!(target: "pkgdev::build", "[build] Distribution type: {}", dist_str);

    match distribution_type {
        gate::DistributionType::Tarbball => {
            tarball::make_release_tarball(&wks, &component)?;
        }
        gate::DistributionType::IPS => {
            // Resolve repository path using the manager and CLI overrides
            let repo_path =
                repo_mgr.resolve(repo_override_path.clone(), repo_override_context.clone())?;
            tracing::info!(target: "pkgdev::build", "[build] Repo resolved to: {} (override path: {:?}, override context: {:?})", repo_path.display(), repo_override_path.as_ref().map(|p| p.display().to_string()), repo_override_context);
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
    tracing::info!(target: "pkgdev::ips", "[ips] Begin IPS actions for '{}' using repo {}", pkg.get_name(), repo_path.display());
    ips::run_generate_filelist(wks, pkg).wrap_err("generating file list failed")?;
    tracing::info!(target: "pkgdev::ips", "[ips] Filelist generated");

    let mut manifests = ips::generate_manifest_files(wks, pkg, gate, transform_include_dir)
        .wrap_err("mogrify failed")?;
    tracing::info!(target: "pkgdev::ips", "[ips] Manifest files generated: {} entries", manifests.len());

    ips::run_generate_pkgdepend(wks, &mut manifests, repo_path)
        .wrap_err("failed to generate dependency entries")?;
    tracing::info!(target: "pkgdev::ips", "[ips] Dependency entries generated");

    ips::run_resolve_dependencies(wks, &mut manifests)
        .wrap_err("failed to resolve dependencies")?;
    tracing::info!(target: "pkgdev::ips", "[ips] Dependencies resolved");

    ips::run_lint(wks, manifests.as_slice()).wrap_err("lint failed")?;
    tracing::info!(target: "pkgdev::ips", "[ips] Lint completed");

    let publisher = gate.clone().unwrap_or_default().publisher;
    tracing::info!(target: "pkgdev::ips", "[ips] Ensuring repository exists at {} with publisher '{}'", repo_path.display(), publisher);
    ips::ensure_repo_with_publisher_exists(repo_path, &publisher)
        .wrap_err("failed to ensure repository exists")?;

    tracing::info!(target: "pkgdev::ips", "[ips] Publishing to repo {} with publisher '{}'", repo_path.display(), publisher);
    ips::publish(wks, pkg, &publisher, manifests.as_slice(), repo_path)
        .wrap_err("package publish failed")?;

    tracing::info!(target: "pkgdev::ips", "[ips] IPS actions completed for '{}'", pkg.get_name());
    Ok(())
}
