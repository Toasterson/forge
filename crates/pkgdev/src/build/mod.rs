mod script;
mod util;
mod automake;
mod compile;
mod install;
mod tarball;
mod ips;

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
    stop_on_step: Option<BuildSteps>,

    #[arg(long, default_value = "false")]
    no_clean: bool,

    #[arg(long, default_value = "false")]
    archive_clean: bool,

    #[arg(short = 'I', long = "include")]
    transform_include_dir: Option<PathBuf>,
}

use std::{
    path::PathBuf
};

use crate::sources::{download_sources, unpack};
use automake::build_using_automake;
use component::Component;
use component::SourceSection;
use config::Settings;
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

pub async fn run_build(component: &Component, gate: &Option<Gate>, wks: &Workspace, settings: &Settings, args: &BuildArgs) -> Result<()> {

    let transform_include_dir = args.transform_include_dir.clone().map(|p| match p.canonicalize() {
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
            .wrap_err("could not clean the download directory")?;
        std::fs::remove_dir_all(wks.get_or_create_build_dir()?)
            .into_diagnostic()
            .wrap_err("could not clean the build directory")?;
        std::fs::remove_dir_all(wks.get_or_create_prototype_dir()?)
            .into_diagnostic()
            .wrap_err("could not clean the prototype directory")?;
        std::fs::remove_dir_all(wks.get_or_create_manifest_dir()?)
            .into_diagnostic()
            .wrap_err("could not clean the manifest directory")?;
    }

    let sources: Vec<SourceSection> = component.recipe.sources.clone();

    download_sources(component, wks, args.archive_clean).await
        .wrap_err("download and verify failed")?;

    if let Some(stop_on_step) = &args.stop_on_step {
        if stop_on_step == &BuildSteps::Download {
            return Ok(());
        }
    }

    unpack::unpack_sources(
        &component,
        &wks,
        sources.as_slice(),
    )
        .wrap_err("unpack step failed")?;

    if let Some(stop_on_step) = &args.stop_on_step {
        if stop_on_step == &BuildSteps::Unpack {
            return Ok(());
        }
    }

    build_package_sources(&wks, &component, &settings)
        .wrap_err("configure step failed")?;

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
        .distribution_type.clone();


    match distribution_type {
        gate::DistributionType::Tarbball => {
            tarball::make_release_tarball(&wks, &component)?;
        }
        gate::DistributionType::IPS => {
            run_ips_actions(
                &wks,
                &component,
                gate,
                transform_include_dir,
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
) -> Result<()> {
    ips::run_generate_filelist(wks, pkg).wrap_err("generating file list failed")?;
    ips::run_mogrify(wks, pkg, gate, transform_include_dir)
        .wrap_err("mogrify failed")?;
    ips::run_generate_pkgdepend(wks, pkg).wrap_err("failed to generate dependency entries")?;
    ips::run_resolve_dependencies(wks, pkg).wrap_err("failed to resolve dependencies")?;
    ips::run_lint(wks, pkg).wrap_err("lint failed")?;

    let publisher = gate.clone().unwrap_or_default().publisher;
    ips::ensure_repo_with_publisher_exists(&publisher)
        .wrap_err("failed to ensure repository exists")?;
    ips::publish_package(wks, pkg, &publisher).wrap_err("package publish failed")?;
    Ok(())
}