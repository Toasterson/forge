use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cargo_metadata::{MetadataCommand, Package, TargetKind};
use component::Component;
use miette::{IntoDiagnostic, Result, WrapErr};
use workspace::Workspace;

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let mode = meta.permissions().mode();
            return meta.is_file() && (mode & 0o111 != 0);
        }
        false
    }
    #[cfg(not(unix))]
    {
        // On non-unix, assume files without extension or with .exe are executables
        path.is_file()
            && path
                .extension()
                .map(|e| e == "exe" || e == "")
                .unwrap_or(true)
    }
}

fn gather_bin_targets_for_dir(
    root: &Path,
) -> miette::Result<(cargo_metadata::Metadata, Vec<(Package, Vec<String>)>)> {
    let mut cmd = MetadataCommand::new();
    cmd.current_dir(root);
    let meta = cmd
        .exec()
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to run cargo metadata in {}", root.display()))?;

    let root_can = root
        .canonicalize()
        .into_diagnostic()
        .wrap_err("failed to canonicalize cargo project root")?;

    let mut result: Vec<(Package, Vec<String>)> = Vec::new();
    for p in &meta.packages {
        let manifest_dir = PathBuf::from(p.manifest_path.as_str())
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let manifest_dir_can = manifest_dir.canonicalize().unwrap_or(manifest_dir.clone());
        if !manifest_dir_can.starts_with(&root_can) {
            continue;
        }
        let bin_names: Vec<String> = p
            .targets
            .iter()
            .filter(|t| t.kind.iter().any(|k| *k == TargetKind::Bin))
            .map(|t| t.name.clone())
            .collect();
        if !bin_names.is_empty() {
            result.push((p.clone(), bin_names));
        }
    }

    Ok((meta, result))
}

pub fn build_and_stage_cargo(wks: &Workspace, component: &Component) -> Result<()> {
    let root = component.get_path();

    // Build using cargo --release
    tracing::info!(target: "pkgdev::cargo", "[cargo] Building project at {}", root.display());
    let mut build_cmd = Command::new("cargo");
    build_cmd.current_dir(root);
    build_cmd.arg("build");
    build_cmd.arg("--release");
    build_cmd.stdout(Stdio::inherit());
    build_cmd.stderr(Stdio::inherit());
    let status = build_cmd
        .status()
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to spawn cargo build in {}", root.display()))?;
    if !status.success() {
        return Err(miette::miette!("cargo build failed"));
    }

    // Determine binaries to stage
    let (meta, bins) = gather_bin_targets_for_dir(root)?;

    let target_dir = meta.target_directory.clone();
    let target_dir_path = PathBuf::from(target_dir.as_std_path());
    let release_dir = target_dir_path.join("release");

    let usr_bin = wks
        .get_or_create_prototype_dir()
        .wrap_err("failed to get prototype dir")?
        .join("usr")
        .join("bin");
    fs::create_dir_all(&usr_bin)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to create {}", usr_bin.display()))?;

    let mut staged_any = false;
    for (_pkg, names) in bins {
        for name in names {
            let bin_path = release_dir.join(&name);
            if !bin_path.exists() {
                // Sometimes Cargo produces hyphen to underscore differences; also try with hyphen replaced
                let alt = release_dir.join(name.replace('-', "_"));
                if alt.exists() {
                    tracing::warn!(target: "pkgdev::cargo", "[cargo] expected binary {} not found; using {}", bin_path.display(), alt.display());
                    stage_file(&alt, &usr_bin)?;
                    staged_any = true;
                    continue;
                }
                tracing::warn!(target: "pkgdev::cargo", "[cargo] built binary not found: {}", bin_path.display());
                continue;
            }
            stage_file(&bin_path, &usr_bin)?;
            staged_any = true;
        }
    }

    if !staged_any {
        // As a fallback, copy all executable files from release dir
        if release_dir.exists() {
            for entry in fs::read_dir(&release_dir).into_diagnostic()? {
                let e = entry.into_diagnostic()?;
                let p = e.path();
                if is_executable(&p) {
                    stage_file(&p, &usr_bin)?;
                    staged_any = true;
                }
            }
        }
    }

    if !staged_any {
        return Err(miette::miette!(
            "no binaries staged from cargo build; ensure the project has [[bin]] targets or a binary crate"
        ));
    }

    tracing::info!(target: "pkgdev::cargo", "[cargo] Staged binaries into {}", usr_bin.display());
    Ok(())
}

fn stage_file(src: &Path, usr_bin: &Path) -> Result<()> {
    let dest =
        usr_bin
            .join(src.file_name().ok_or_else(|| {
                miette::miette!("missing file name for binary {}", src.display())
            })?);
    fs::copy(src, &dest)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to copy {} to {}", src.display(), dest.display()))?;
    // Ensure executable bit on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest).into_diagnostic()?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms).into_diagnostic()?;
    }
    Ok(())
}
