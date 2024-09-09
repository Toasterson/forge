use std::{
    collections::HashMap,
    fs::DirBuilder,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use component::{Component, ConfigureBuildSection, ScriptBuildSection};
use miette::{IntoDiagnostic, Result, WrapErr};
use workspace::Workspace;

pub fn build_using_scripts(
    wks: &Workspace,
    pkg: &Component,
    build_section: &ScriptBuildSection,
    settings: &Settings,
) -> Result<()> {
    let build_dir = wks.get_or_create_build_dir()?;
    let unpack_name = derive_source_name(
        pkg.package_document.name.clone(),
        &pkg.package_document.sources[0],
    );
    let unpack_path = build_dir.join(&unpack_name);
    std::env::set_current_dir(&unpack_path).into_diagnostic()?;

    for script in &build_section.scripts {
        let status = Command::new(pkg.get_path().join(&script.name))
            .stdout(Stdio::inherit())
            .env(
                "PROTO_DIR",
                wks.get_or_create_prototype_dir()
                    .into_diagnostic()?
                    .into_os_string(),
            )
            .env("UNPACK_DIR", &unpack_path.clone().into_os_string())
            .env("PATH", settings.get_search_path().join(":"))
            .status()
            .into_diagnostic()?;

        if status.success() {
            println!(
                "Successfully ran script {} in package {}",
                script.name,
                pkg.get_name()
            );
        } else {
            return Err(miette::miette!(format!(
                "Could not run script {} in package {}",
                script.name,
                pkg.get_name()
            )));
        }

        if let Some(prototype_dir) = &script.prototype_dir {
            println!(
                "Copying prototype directory {} to workspace prototype directory",
                &prototype_dir.display()
            );

            let mut copy_options = fs_extra::dir::CopyOptions::default();
            copy_options.overwrite = true;
            copy_options.content_only = true;

            if let Some(prefix) = &pkg.package_document.prefix {
                let prefix = if prefix.starts_with("/") {
                    &prefix[1..]
                } else {
                    prefix.as_str()
                };

                let target_path = wks.get_or_create_prototype_dir()?.join(prefix);
                if !target_path.exists() {
                    DirBuilder::new()
                        .recursive(true)
                        .create(&target_path)
                        .into_diagnostic()?;
                    println!("Creating target path {}", target_path.display());
                }

                let src_path = unpack_path.join(&prototype_dir);

                println!("src: {}", &src_path.display());
                println!("exists?: {}", src_path.exists());
                println!("target: {}", &target_path.display());
                println!("exists?: {}", &target_path.exists());

                fs_extra::dir::copy(&src_path, &target_path, &copy_options).into_diagnostic()?;
            } else {
                fs_extra::dir::copy(
                    unpack_path.join(&prototype_dir),
                    wks.get_or_create_prototype_dir()?,
                    &copy_options,
                )
                    .into_diagnostic()?;
            }
        }
    }

    for install_directive in &build_section.install_directives {
        let target_path = if let Some(prefix) = &pkg.package_document.prefix {
            let prefix = if prefix.starts_with("/") {
                &prefix[1..]
            } else {
                prefix.as_str()
            };

            wks.get_or_create_prototype_dir()?
                .join(&prefix)
                .join(&install_directive.target)
        } else {
            wks.get_or_create_prototype_dir()?
                .join(&install_directive.target)
        };
        println!("Copying directory to prototype dir");
        println!("Target Path: {}", target_path.display());
        let src_full_path = unpack_path.join(&install_directive.src);
        println!("Source Path: {}", src_full_path.display());

        if let Some(pattern) = &install_directive.pattern {
            if !target_path.exists() {
                DirBuilder::new()
                    .recursive(true)
                    .create(&target_path)
                    .into_diagnostic()?;
                println!("Creating target dir");
            }
            let mut copy_options = fs_extra::file::CopyOptions::default();
            copy_options.overwrite = true;
            let files = file_matcher::FilesNamed::regex(pattern)
                .within(&src_full_path)
                .find()
                .into_diagnostic()?;
            println!("Copying via rsync");
            copy_with_rsync(wks, &src_full_path, &target_path, files)?;
        } else if let Some(fmatch) = &install_directive.fmatch {
            if !target_path.exists() {
                DirBuilder::new()
                    .recursive(true)
                    .create(&target_path)
                    .into_diagnostic()?;
                println!("Creating target dir");
            }
            let files = file_matcher::FilesNamed::wildmatch(fmatch)
                .within(&src_full_path)
                .find()
                .into_diagnostic()?;
            println!("Copying via rsync");
            copy_with_rsync(wks, &src_full_path, &target_path, files)?;
        } else {
            if src_full_path.is_file() {
                if !target_path.exists() {
                    DirBuilder::new()
                        .recursive(true)
                        .create(
                            &target_path
                                .parent()
                                .ok_or(miette::miette!("path has no parent directory"))?,
                        )
                        .into_diagnostic()?;
                    println!("Creating target dir");
                }
                let mut copy_options = fs_extra::file::CopyOptions::default();
                copy_options.overwrite = true;
                fs_extra::file::copy(src_full_path, target_path, &copy_options)
                    .into_diagnostic()?;
            } else {
                if !target_path.exists() {
                    DirBuilder::new()
                        .recursive(true)
                        .create(&target_path)
                        .into_diagnostic()?;
                    println!("Creating target dir");
                }
                let mut copy_options = fs_extra::dir::CopyOptions::default();
                copy_options.overwrite = true;
                copy_options.content_only = true;
                fs_extra::dir::copy(src_full_path, target_path, &copy_options).into_diagnostic()?;
            }
        }
        println!("Copy suceeded");
    }

    println!("Build for package {} finished", pkg.get_name());

    Ok(())
}