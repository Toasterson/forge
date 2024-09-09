mod script;
mod util;
mod automake;
mod unpack;
mod path;
mod compile;
mod install;
mod tarball;
mod ips;
mod download;

use clap::{Subcommand, ValueEnum};
use workspace::Workspace;

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuildSteps {
    Download,
    Unpack,
    Build,
    Pack,
    Publish,
}

#[derive(Debug, Subcommand)]
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
    collections::HashMap,
    fs::DirBuilder,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use component::{Component, ConfigureBuildSection, ScriptBuildSection};
use miette::{IntoDiagnostic, Result, WrapErr};
use automake::build_using_automake;
use script::build_using_scripts;
use config::Settings;
use component::SourceSection;
use gate::Gate;

pub fn derive_source_name(package_name: String) -> String {
    package_name.replace("/", "_")
}

pub fn build_package_sources(wks: &Workspace, pkg: &Component, settings: &Settings) -> Result<()> {
    for section in pkg.recipe.build_sections {
        if let Some(c) = section.configure {
            build_using_automake(wks, pkg, &c, settings)?;
        } else if let Some(_) = section.cmake {
            unimplemented!();
        } else if let Some(_) = section.meson {
            unimplemented!();
        } else if let Some(script) = section.script {
            build_using_scripts(wks, pkg, &script, settings)?;
        }
    }

    Ok(())
}

pub fn run_build<P: AsRef<Path>>(component_path: Option<P>, gate_path: Option<P>, workspace_path: Option<P>, args: &BuildArgs) -> Result<()> {
    let settings = Settings::open()?;

    let wks = if let Some(wks_path) = workspace_path {
        settings.get_workspace_from(wks_path.as_path())?
    } else {
        settings.get_current_wks()?
    };

    let transform_include_dir = args.transform_include_dir.clone().map(|p| match p.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            println!(
                "could not cannonicalize {} due to {} continuing ignoring and continuing",
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

    let (component, gate_data) = if let Some(gate_path) = gate_path {
        let gate_data = Gate::new(&gate_path).wrap_err("could not open gate data")?;

        let path = if let Some(package) = &component_path {
            let name = if package.contains("/") {
                package.rsplit_once('/').unwrap().1
            } else {
                package.as_str()
            };
            gate_path
                .parent()
                .unwrap_or(Path::new("./"))
                .join("packages")
                .join(name)
        } else {
            Path::new("./").to_path_buf()
        };

        let path = path.canonicalize().into_diagnostic().wrap_err(format!(
            "Can not canonicalize path to package {}",
            path.display()
        ))?;

        let mut component =
            Component::open_local(path).wrap_err("could not open package.kdl of package")?;

        (component, Some(gate_data))
    } else {
        let path = if let Some(package) = component_path {
            let name = if package.contains("/") {
                package.split_once('/').unwrap().1
            } else {
                package.as_str()
            };
            Path::new("./packages").join(name)
        } else {
            Path::new("./").to_path_buf()
        };

        let path = path.canonicalize().into_diagnostic().wrap_err(format!(
            "Can not canonicalize path to package {}",
            path.display()
        ))?;

        (
            Component::open_local(path).wrap_err("could not open package.kdl of package")?,
            None,
        )
    };

    let sources: Vec<SourceSection> = component.recipe.sources.clone();

    download::download_and_verify(&wks, sources.as_slice(), args.archive_clean)
        .wrap_err("download and verify failed")?;

    if let Some(stop_on_step) = &args.stop_on_step {
        if stop_on_step == &BuildSteps::Download {
            return Ok(());
        }
    }

    unpack::unpack_sources(
        &wks,
        component.recipe.name.clone(),
        component.get_path(),
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

    if let Some(gate_data) = gate_data {
        if let Some(distribution) = &gate_data.distribution {
            match distribution.distribution_type {
                gate::DistributionType::Tarbball => {
                    tarball::make_release_tarball(&wks, &component)?;
                }
                gate::DistributionType::IPS => {
                    run_ips_actions(
                        &wks,
                        &component,
                        Some(gate_data),
                        transform_include_dir,
                    )?;
                }
            }
        } else {
            run_ips_actions(
                &wks,
                &component,
                Some(gate_data),
                transform_include_dir,
            )?;
        }
    } else {
        run_ips_actions(
            &wks,
            &component,
            gate_data.clone(),
            transform_include_dir,
        )?;
    }

    if let Some(stop_on_step) = &args.stop_on_step {
        if stop_on_step == &BuildSteps::Pack {
            return Ok(());
        }
    }

    Ok(())
}

fn run_ips_actions(
    wks: &Workspace,
    pkg: &Component,
    gate_data: Option<Gate>,
    transform_include_dir: Option<PathBuf>,
) -> Result<()> {
    ips::run_generate_filelist(wks, pkg).wrap_err("generating filelist failed")?;
    ips::run_mogrify(wks, pkg, gate_data.clone(), transform_include_dir)
        .wrap_err("mogrify failed")?;
    ips::run_generate_pkgdepend(wks, pkg).wrap_err("failed to generate dependency entries")?;
    ips::run_resolve_dependencies(wks, pkg).wrap_err("failed to resolve dependencies")?;
    ips::run_lint(wks, pkg).wrap_err("lint failed")?;

    let publisher = &gate_data.unwrap_or(Gate::default()).publisher;
    ips::ensure_repo_with_publisher_exists(&publisher)
        .wrap_err("failed to ensure repository exists")?;
    ips::publish_package(wks, pkg, &publisher).wrap_err("package publish failed")?;
    Ok(())
}