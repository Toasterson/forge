use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use miette::{IntoDiagnostic, Result};
use workspace::Workspace;

pub fn copy_with_rsync<P: AsRef<Path>>(
    wks: &Workspace,
    from: P,
    to: P,
    file_list: Vec<PathBuf>,
) -> Result<()> {
    //write file_list to known location into file
    let contents_file_path = wks.get_or_create_build_dir()?.join("install_file_list.txt");

    let src_path_string = from.as_ref().to_string_lossy().to_string();

    let file_list = file_list
        .into_iter()
        .map(|p| {
            p.to_string_lossy()
                .to_string()
                .replace(&src_path_string, "")
        })
        .collect::<Vec<String>>()
        .join("\n");

    println!("writing file list:\n{}", &file_list);

    let mut contents_file = std::fs::File::create(&contents_file_path).into_diagnostic()?;
    contents_file
        .write_all(&mut file_list.as_bytes())
        .into_diagnostic()?;
    drop(contents_file);
    let contents_file_arg = format!(
        "--files-from={}",
        contents_file_path.to_string_lossy().to_string()
    );

    // point rsync command to it to copy over selected files
    let rsync_status = Command::new("rsync")
        .arg("-avp")
        .arg(&contents_file_arg)
        .arg(path_2_string(from))
        .arg(path_2_string(to))
        .stdout(Stdio::inherit())
        .status()
        .into_diagnostic()?;

    if rsync_status.success() {
        Ok(())
    } else {
        Err(miette::miette!("failed to copy directories with rsync"))
    }
}

pub fn path_2_string<P: AsRef<Path>>(path: P) -> String {
    path.as_ref().to_string_lossy().to_string()
}

#[inline(never)]
pub fn can_expand_value(value: &str) -> bool {
    value.contains("$")
}

#[inline(never)]
pub fn expand_env(value: &str) -> Result<String> {
    if can_expand_value(value) {
        shellexpand::env(value)
            .map(|r| r.to_string())
            .into_diagnostic()
    } else {
        Ok(value.to_string())
    }
}
