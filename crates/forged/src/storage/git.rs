use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;

use miette::{Context, IntoDiagnostic};

use crate::settings::GitStorageConfig;

#[derive(Clone, Debug)]
pub struct RepoManager {
    cfg: GitStorageConfig,
}

impl RepoManager {
    pub fn new(cfg: GitStorageConfig) -> Self {
        Self { cfg }
    }

    fn fs_root(&self) -> PathBuf {
        let root = self
            .cfg
            .root
            .clone()
            .unwrap_or_else(|| "./data/repos".to_string());
        PathBuf::from(root)
    }

    fn repo_path(&self, component_id: &str) -> PathBuf {
        // One repo per component id; keep it simple and readable on disk
        self.fs_root().join(component_id)
    }

    fn bare_repo_path(&self, component_id: &str) -> PathBuf {
        self.fs_root().join(format!("{}.git", component_id))
    }

    /// Public helper to get the on-disk directory for a component's repository (non-bare worktree).
    pub fn repo_dir(&self, component_id: &str) -> PathBuf {
        self.repo_path(component_id)
    }

    /// Public helper to get the on-disk directory of the bare repository used for SmartPush.
    pub fn repo_bare_dir(&self, component_id: &str) -> PathBuf {
        self.bare_repo_path(component_id)
    }

    /// After receiving an incoming pack file, rename it to its canonical
    /// name `pack-<trailer-sha1>.pack` within the bare repo's objects/pack dir.
    /// Returns the final path.
    pub fn finalize_incoming_pack(
        &self,
        component_id: &str,
        incoming_pack: &Path,
    ) -> miette::Result<PathBuf> {
        let pack_dir = self
            .repo_bare_dir(component_id)
            .join("objects")
            .join("pack");
        std::fs::create_dir_all(&pack_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("create pack dir {}", pack_dir.display()))?;

        // Read the last 20 bytes which are the trailing checksum per pack format.
        let mut f = std::fs::File::open(incoming_pack)
            .into_diagnostic()
            .wrap_err_with(|| format!("open incoming pack {}", incoming_pack.display()))?;
        let meta = f
            .metadata()
            .into_diagnostic()
            .wrap_err("stat incoming pack")?;
        let len = meta.len();
        if len < 20 {
            return Err(miette::miette!("incoming pack too small ({} bytes)", len));
        }
        f.seek(SeekFrom::End(-20))
            .into_diagnostic()
            .wrap_err("seek pack trailer")?;
        let mut trailer = [0u8; 20];
        f.read_exact(&mut trailer)
            .into_diagnostic()
            .wrap_err("read pack trailer")?;
        drop(f);

        let mut hex = String::with_capacity(40);
        for b in trailer.iter() {
            use std::fmt::Write as _;
            let _ = write!(&mut hex, "{:02x}", b);
        }
        let final_path = pack_dir.join(format!("pack-{}.pack", hex));
        std::fs::rename(incoming_pack, &final_path)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "rename incoming pack {} -> {}",
                    incoming_pack.display(),
                    final_path.display()
                )
            })?;
        Ok(final_path)
    }

    pub fn ensure_repo(&self, component_id: &str) -> miette::Result<bool> {
        // Only FS mode supported in MVP
        let mode = self.cfg.mode.clone().unwrap_or_else(|| "fs".into());
        match mode.as_str() {
            "fs" => self.ensure_repo_fs(component_id),
            "s3" => {
                // Future: materialize working copy, sync with S3
                Err(miette::miette!("S3 repo storage not implemented yet"))
            }
            other => Err(miette::miette!("unsupported repo storage mode: {}", other)),
        }
    }

    fn ensure_repo_fs(&self, component_id: &str) -> miette::Result<bool> {
        let repo_dir = self.repo_path(component_id);
        if repo_dir.join(".git").exists() {
            return Ok(false);
        }
        std::fs::create_dir_all(&repo_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("create repo dir {}", repo_dir.display()))?;
        // Initialize with git CLI for simplicity
        let status = Command::new("git")
            .arg("init")
            .current_dir(&repo_dir)
            .status()
            .into_diagnostic()
            .wrap_err("spawn git init")?;
        if !status.success() {
            return Err(miette::miette!("git init failed in {}", repo_dir.display()));
        }
        // Set a default identity if none configured to allow committing
        let _ = Command::new("git")
            .args(["config", "user.name", "forged"])
            .current_dir(&repo_dir)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.email", "forged@localhost"])
            .current_dir(&repo_dir)
            .status();
        // Allow pushes to the current branch in this non-bare repository by updating the worktree
        let _ = Command::new("git")
            .args(["config", "receive.denyCurrentBranch", "updateInstead"])
            .current_dir(&repo_dir)
            .status();
        Ok(true)
    }

    pub fn ensure_bare_repo(&self, component_id: &str) -> miette::Result<bool> {
        let mode = self.cfg.mode.clone().unwrap_or_else(|| "fs".into());
        match mode.as_str() {
            "fs" => self.ensure_bare_repo_fs(component_id),
            "s3" => Err(miette::miette!("S3 repo storage not implemented yet")),
            other => Err(miette::miette!("unsupported repo storage mode: {}", other)),
        }
    }

    fn ensure_bare_repo_fs(&self, component_id: &str) -> miette::Result<bool> {
        let bare_dir = self.bare_repo_path(component_id);
        if bare_dir.exists() && bare_dir.join("HEAD").exists() {
            return Ok(false);
        }
        if let Some(parent) = bare_dir.parent() {
            std::fs::create_dir_all(parent)
                .into_diagnostic()
                .wrap_err_with(|| format!("create bare repo parent dir {}", parent.display()))?;
        }
        let status = Command::new("git")
            .arg("init")
            .arg("--bare")
            .arg(&bare_dir)
            .status()
            .into_diagnostic()
            .wrap_err("spawn git init --bare")?;
        if !status.success() {
            return Err(miette::miette!(
                "git init --bare failed in {}",
                bare_dir.display()
            ));
        }
        Ok(true)
    }

    /// Find the most recent pack file for a component's bare repository.
    /// Preference order: latest modification time; fallback to lexicographic.
    pub fn latest_pack_path(&self, component_id: &str) -> miette::Result<PathBuf> {
        let pack_dir = self
            .repo_bare_dir(component_id)
            .join("objects")
            .join("pack");
        if !pack_dir.exists() {
            return Err(miette::miette!("no pack directory: {}", pack_dir.display()));
        }
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        for entry in std::fs::read_dir(&pack_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("read pack dir {}", pack_dir.display()))?
        {
            let entry = entry.into_diagnostic()?;
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) != Some("pack") {
                continue;
            }
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                if !name.starts_with("pack-") {
                    continue;
                }
            }
            let meta = entry.metadata().into_diagnostic()?;
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            best = match best.take() {
                None => Some((mtime, p)),
                Some((prev_time, prev_path)) => {
                    if mtime > prev_time {
                        Some((mtime, p))
                    } else {
                        Some((prev_time, prev_path))
                    }
                }
            };
        }
        if let Some((_t, path)) = best {
            Ok(path)
        } else {
            Err(miette::miette!("no pack files in {}", pack_dir.display()))
        }
    }

    pub fn put_version_package_kdl(
        &self,
        component_id: &str,
        version: &str,
        package_kdl: &[u8],
    ) -> miette::Result<String> {
        // FS-only MVP
        let mode = self.cfg.mode.clone().unwrap_or_else(|| "fs".into());
        match mode.as_str() {
            "fs" => self.put_version_fs(component_id, version, package_kdl),
            "s3" => Err(miette::miette!("S3 repo storage not implemented yet")),
            other => Err(miette::miette!("unsupported repo storage mode: {}", other)),
        }
    }

    fn put_version_fs(
        &self,
        component_id: &str,
        version: &str,
        package_kdl: &[u8],
    ) -> miette::Result<String> {
        self.ensure_repo_fs(component_id)?;
        let repo_dir = self.repo_path(component_id);
        let pkg_path = repo_dir.join("package.kdl");
        std::fs::write(&pkg_path, package_kdl)
            .into_diagnostic()
            .wrap_err_with(|| format!("write {}", pkg_path.display()))?;

        let add_ok = Command::new("git")
            .args(["add", "."]) // add all files for simplicity
            .current_dir(&repo_dir)
            .status()
            .into_diagnostic()
            .wrap_err("spawn git add")?;
        if !add_ok.success() {
            return Err(miette::miette!("git add failed in {}", repo_dir.display()));
        }

        let msg = format!("chore: release {}", version);
        let commit_ok = Command::new("git")
            .args(["commit", "-m", &msg, "--allow-empty"])
            .current_dir(&repo_dir)
            .status()
            .into_diagnostic()
            .wrap_err("spawn git commit")?;
        if !commit_ok.success() {
            return Err(miette::miette!(
                "git commit failed in {}",
                repo_dir.display()
            ));
        }

        // Create/update tag v<version>
        let tag_name = format!("v{}", version);
        let _ = Command::new("git")
            .args(["tag", "-f", &tag_name])
            .current_dir(&repo_dir)
            .status();

        // Get commit id
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo_dir)
            .output()
            .into_diagnostic()
            .wrap_err("spawn git rev-parse")?;
        if !output.status.success() {
            return Err(miette::miette!(
                "git rev-parse failed in {}",
                repo_dir.display()
            ));
        }
        let mut sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if sha.is_empty() {
            sha = "unknown".into();
        }
        Ok(sha)
    }
}
