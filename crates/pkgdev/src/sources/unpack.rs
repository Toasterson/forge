use std::{
    fs::{read_dir, DirBuilder},
    path::{Path, PathBuf},
    process::Command,
};

use crate::sources::derive_source_name;
use crate::sources::path::add_extension;
use component::{Component, SourceSection};
use miette::{IntoDiagnostic, Result, WrapErr};
use workspace::Workspace;

pub fn unpack_sources(
    component: &Component,
    wks: &Workspace,
    sources: &[SourceSection],
) -> Result<()> {
    let build_dir = wks.get_or_create_build_dir()?;
    std::env::set_current_dir(&build_dir).into_diagnostic()?;
    let package_name = component.recipe.name.clone();

    for (source_idx, source) in sources.into_iter().enumerate() {
        let unpack_name = derive_source_name(package_name.clone());
        let unpack_path = build_dir.join(&unpack_name);

        for (node_idx, src) in source.sources.clone().into_iter().enumerate() {
            match src {
                component::SourceNode::Archive(archive) => {
                    let src_url: url::Url = archive.src.parse().into_diagnostic()?;
                    let local_file = wks.get_file_path(&src_url)?;
                    archive_unpack(&local_file, &unpack_path, &package_name)?;
                }
                component::SourceNode::Git(git_src) => {
                    let file_name = add_extension(git_src.get_repo_prefix(), "tar.gz");
                    let archive_download_path = wks.get_or_create_download_dir()?.join(file_name);
                    if node_idx == 0 && source_idx == 0 {
                        archive_unpack(&archive_download_path, &unpack_path, &package_name)?;
                    } else {
                        if let Some(unpack_name) = git_src.directory {
                            let unpack_path = build_dir.join(unpack_name);
                            archive_unpack(&archive_download_path, &unpack_path, &package_name)?;
                        } else {
                            return Err(miette::miette!(
                                "directory property is only optional in the first git source"
                            ));
                        }
                    }
                }
                component::SourceNode::File(file) => {
                    let src_path = file.get_bundle_path(component.get_path());
                    let final_path = unpack_path.join(file.get_target_path());

                    if let Some(final_dir) = final_path.parent() {
                        if !final_dir.exists() {
                            DirBuilder::new()
                                .recursive(true)
                                .create(&final_dir)
                                .into_diagnostic()?;
                        }
                    }

                    println!(
                        "Copying file {} to {}",
                        src_path.to_string_lossy().to_string(),
                        final_path.to_string_lossy().to_string()
                    );

                    std::fs::copy(src_path, final_path).into_diagnostic()?;
                }
                component::SourceNode::Patch(patch) => {
                    let src_path = patch
                        .get_bundle_path(component.get_path())
                        .to_string_lossy()
                        .to_string();
                    let unpack_arg = unpack_path.to_string_lossy().to_string();
                    let mut patch_cmd = Command::new("gpatch");
                    patch_cmd.arg("-d");
                    patch_cmd.arg(&unpack_arg);
                    if let Some(drop_directories) = patch.drop_directories {
                        let strip_arg = format!("-p{}", drop_directories);
                        patch_cmd.arg(&strip_arg);
                    }
                    patch_cmd.arg("-i");
                    patch_cmd.arg(&src_path);

                    let status = patch_cmd
                        .status()
                        .into_diagnostic()
                        .wrap_err("could not run gpatch")?;

                    if !status.success() {
                        return Err(miette::miette!("failed to patch sources"));
                    }
                }
                component::SourceNode::Overlay(overlay) => {
                    println!("Overlaying directory {}", unpack_path.display());
                    let src_path = overlay.get_bundle_path(component.get_path());
                    let final_path = unpack_path.clone();
                    let mut copy_opts = fs_extra::dir::CopyOptions::new();
                    copy_opts.overwrite = true;
                    copy_opts.content_only = true;
                    fs_extra::dir::copy(&src_path, final_path, &copy_opts).into_diagnostic()?;
                }
                component::SourceNode::Directory(directory) => {
                    println!(
                        "Copying directory {} into build workspace",
                        directory.get_name()
                    );
                    let src_path = directory.get_bundle_path(component.get_path());
                    let final_path = build_dir.join(directory.get_target_path());
                    println!("{} -> {}", src_path.display(), final_path.display());
                    DirBuilder::new().create(&final_path).into_diagnostic()?;
                    let mut copy_opts = fs_extra::dir::CopyOptions::new();
                    copy_opts.content_only = true;
                    fs_extra::dir::copy(&src_path, final_path, &copy_opts).into_diagnostic()?;
                }
            }
        }
    }
    Ok(())
}

fn archive_unpack<P: AsRef<Path>>(local_file: P, final_path: P, name: &str) -> Result<()> {
    let local_file = local_file.as_ref();
    let final_path = final_path.as_ref();
    if !local_file.exists() {
        return Err(miette::miette!(format!(
            "archive path {} does not exist cannot unpack",
            local_file.display()
        )));
    }

    if final_path.exists() {
        println!("Archive for {} already extracted skipping", name);
        return Ok(());
    }

    let tmp_dir_path = Path::new("tmp.unpack");
    if !tmp_dir_path.exists() {
        DirBuilder::new().create(tmp_dir_path).into_diagnostic()?;
    } else {
        std::fs::remove_dir_all(&tmp_dir_path).into_diagnostic()?;
        DirBuilder::new().create(tmp_dir_path).into_diagnostic()?;
    }

    use compress_tools::*;
    use std::fs::File;

    let archive_file = File::open(local_file).into_diagnostic()?;

    uncompress_archive(archive_file, tmp_dir_path, Ownership::Ignore)
        .into_diagnostic()
        .wrap_err("libarchive uncompress")?;

    let extracted_dirs = read_dir(tmp_dir_path)
        .into_diagnostic()?
        .into_iter()
        .filter_map(|e| match e {
            Ok(e) => Some((e.file_name().to_string_lossy().to_string(), e.path())),
            Err(_) => None,
        })
        .collect::<Vec<(String, PathBuf)>>();
    let extracted_dir = extracted_dirs
        .first()
        .ok_or(miette::miette!("no directories extracted"))?;

    println!(
        "extracted_dir={}; final_name={}",
        extracted_dir.0,
        final_path
            .file_name()
            .ok_or(miette::miette!("No filename"))?
            .to_string_lossy()
            .to_string()
    );

    std::fs::rename(&extracted_dir.1, final_path)
        .into_diagnostic()
        .wrap_err("move extracted directories to build path")?;

    std::fs::remove_dir(tmp_dir_path).into_diagnostic()?;

    Ok(())
}
