use std::io::{copy, Cursor};
use std::path::Path;
use std::process::Command;

use miette::{Diagnostic, IntoDiagnostic};
use thiserror::Error;

use component::{ArchiveSource, GitSource, SourceNode};
use gate::Gate;
use workspace::{HasherKind, Workspace};

use crate::sources::path::add_extension;

mod path;

#[derive(Debug, Error, Diagnostic)]
#[error("hash mismatch\nexpected:\t{expected}\nactual:\t{actual}")]
pub struct HashMismatchError {
    expected: String,
    actual: String,
}

pub(crate) fn download_sources<P: AsRef<Path>>(
    component: P,
    _gate: Option<Gate>,
    target_dir: P,
) -> miette::Result<()> {
    let wks = Workspace::new(target_dir)?;
    let component = component::Component::open_local(component)?;
    println!("Loaded component recipe: {}", &component.recipe.name);
    let sources = component.recipe.sources;
    for source in sources {
        for src in source.sources {
            if let SourceNode::Archive(ar) = src {
                println!("Downloading archive: {}", &ar.src);
                match download_archive(&wks, &ar) {
                    Ok(_) => println!("Download finished"),
                    Err(err) => println!("{}\nwill continue with other downloads", err),
                }
            } else if let SourceNode::Git(g) = src {
                println!("Downloading git repo: {}", &g.repository);
                download_git(&wks, &g)?;
            }
        }
    }
    Ok(())
}

fn download_archive(wks: &Workspace, archive: &ArchiveSource) -> miette::Result<()> {
    let response = reqwest::blocking::get(&archive.src).into_diagnostic()?;
    let (hash, hasher_kind) = if let Some(sha256) = &archive.sha256 {
        Ok((sha256.clone(), HasherKind::Sha256))
    } else if let Some(sha512) = &archive.sha512 {
        Ok((sha512.clone(), HasherKind::Sha512))
    } else {
        Err(miette::miette!(
            "no hash specified in the source instruction"
        ))
    }?;

    let mut dest =
        wks.open_or_truncate_local_file(&archive.src.parse().into_diagnostic()?, hasher_kind)?;
    let mut content = Cursor::new(response.bytes().into_diagnostic()?);
    copy(&mut content, &mut dest).into_diagnostic()?;
    let computed_hash = dest.get_hash();
    if hash != computed_hash {
        return Err(HashMismatchError {
            expected: hash,
            actual: computed_hash,
        }
        .into());
    }

    Ok(())
}

fn download_git(wks: &Workspace, git: &GitSource) -> miette::Result<()> {
    let git_prefix = &git.get_repo_prefix();
    let git_repo_path = wks.get_or_create_download_dir()?.join(&git_prefix);
    let archive_path = add_extension(&git_prefix, "tar.gz");

    // Remove file equivalent to rm -f
    std::fs::remove_file(&archive_path).ok();

    if !archive_path.exists() {
        if !git_repo_path.exists() {
            if git.archive.is_some() {
                git_archive_get(wks, &git)?;
            } else {
                git_clone_get(wks, &git)?;
            }
        } else {
            if git.must_stay_as_repo.is_some() {
                println!("Creating Archive of full repo");
                make_git_archive_with_tar(wks, git)?;
            } else {
                println!("Creating git-archive based archive from git");
                make_git_archive(wks, git)?;
            }
        }
    }
    Ok(())
}

fn git_clone_get(wks: &Workspace, git: &GitSource) -> miette::Result<()> {
    let mut git_cmd = Command::new("git");

    let repo_prefix = git.get_repo_prefix();

    git_cmd.current_dir(&wks.get_or_create_download_dir()?);
    git_cmd.arg("clone");
    git_cmd.arg("--single-branch");
    if let Some(tag) = &git.tag {
        git_cmd.arg("--branch");
        git_cmd.arg(tag);
    } else if let Some(branch) = &git.branch {
        git_cmd.arg("--branch");
        git_cmd.arg(branch);
    }
    git_cmd.arg(&git.repository);
    git_cmd.arg(&repo_prefix);

    let status = git_cmd.status().into_diagnostic()?;
    if status.success() {
        println!("Git successfully cloned from remote");
    } else {
        return Err(miette::miette!(format!(
            "Could not git clone {}",
            git.repository
        )));
    }

    if git.must_stay_as_repo.is_some() {
        println!("Creating Archive of full repo");
        make_git_archive_with_tar(wks, git)
    } else {
        println!("Creating git-archive based archive from git");
        make_git_archive(wks, git)
    }
}

fn make_git_archive_with_tar(wks: &Workspace, git: &GitSource) -> miette::Result<()> {
    let repo_prefix = git.get_repo_prefix();

    let mut archive_cmd = Command::new("gtar");
    archive_cmd.current_dir(&wks.get_or_create_download_dir()?);
    archive_cmd.arg("-czf");
    let archive_name_arg = add_extension(&repo_prefix, "tar.gz")
        .to_string_lossy()
        .to_string();
    archive_cmd.arg(&archive_name_arg);
    archive_cmd.arg(&repo_prefix);

    let status = archive_cmd.status().into_diagnostic()?;
    if status.success() {
        println!(
            "Git Archive {}.tar.gz successfully created by way of tar",
            &repo_prefix
        );
        Ok(())
    } else {
        Err(miette::miette!(format!(
            "Could not create archive of {}",
            &repo_prefix
        )))
    }
}

fn make_git_archive(wks: &Workspace, git: &GitSource) -> miette::Result<()> {
    let repo_prefix = git.get_repo_prefix();

    let mut archive_cmd = Command::new("git");
    archive_cmd.current_dir(&wks.get_or_create_download_dir()?.join(&repo_prefix));
    archive_cmd.arg("archive");
    archive_cmd.arg("--format=tar.gz");
    let prefix_arg = format!("--prefix={}/", &repo_prefix);
    let output_arg = format!(
        "--output={}",
        add_extension(&repo_prefix, "tar.gz")
            .to_string_lossy()
            .to_string()
    );
    archive_cmd.arg(&prefix_arg);
    archive_cmd.arg(&output_arg);
    archive_cmd.arg("HEAD");

    let status = archive_cmd.status().into_diagnostic()?;
    if status.success() {
        println!("Git Archive {}.tar.gz successfully created", &repo_prefix);
        Ok(())
    } else {
        Err(miette::miette!(format!(
            "Could not create archive of {}",
            &repo_prefix
        )))
    }
}

fn git_archive_get(wks: &Workspace, git: &GitSource) -> miette::Result<()> {
    let mut git_cmd = Command::new("git");
    let repo_prefix = git.get_repo_prefix();

    let prefix_arg = format!("--prefix={}", &repo_prefix);
    let output_arg = format!(
        "--output={}",
        add_extension(&repo_prefix, "tar.gz")
            .to_string_lossy()
            .to_string()
    );
    let remote_arg = format!("--remote={}", &git.repository);

    git_cmd.current_dir(&wks.get_or_create_download_dir()?);
    git_cmd.arg("archive");
    git_cmd.arg("--format=tar.gz");
    git_cmd.arg(prefix_arg);
    git_cmd.arg(output_arg);
    git_cmd.arg(remote_arg);
    git_cmd.arg("-v");
    if let Some(tag) = &git.tag {
        git_cmd.arg(tag);
    } else if let Some(branch) = &git.branch {
        git_cmd.arg(branch);
    } else {
        git_cmd.arg("HEAD");
    }

    let status = git_cmd.status().into_diagnostic()?;
    if status.success() {
        println!("Archive successfully copied from git remote");
        Ok(())
    } else {
        Err(miette::miette!(format!(
            "Could not get git archive for {}",
            git.repository
        )))
    }
}
