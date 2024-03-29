use std::fs::DirBuilder;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use miette::Diagnostic;
use sha2::Digest;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum WorkspaceError {
    #[error(transparent)]
    #[diagnostic(code(wk::io))]
    IOError(#[from] io::Error),

    #[error("the url {0} is an invalid format it must have a filename at the end")]
    #[diagnostic(code(wk::url::invalid))]
    InvalidURLError(url::Url),
}

type Result<T> = miette::Result<T, WorkspaceError>;

#[derive(Debug)]
pub struct Workspace {
    path: PathBuf,
}

impl Workspace {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        if !path.as_ref().exists() {
            DirBuilder::new().recursive(true).create(path.as_ref())?;
        }
        let full_path = std::fs::canonicalize(path.as_ref())?;
        Ok(Self { path: full_path })
    }

    pub fn get_or_create_download_dir(&self) -> Result<PathBuf> {
        let download_dir = self.path.join("downloads");
        if !download_dir.exists() {
            DirBuilder::new().recursive(true).create(&download_dir)?;
        }
        Ok(download_dir)
    }

    pub fn get_file_path(&self, url: &url::Url) -> Result<PathBuf> {
        let download_dir = self.get_or_create_download_dir()?;
        Ok(download_dir.join(
            Path::new(url.path())
                .file_name()
                .ok_or(WorkspaceError::InvalidURLError(url.clone()))?,
        ))
    }

    pub fn open_local_file(&self, url: &url::Url, hasher_kind: HasherKind) -> Result<DownloadFile> {
        let download_dir = self.get_or_create_download_dir()?;
        let p = download_dir.join(self.get_file_path(url)?);
        DownloadFile::new(p, hasher_kind)
    }

    pub fn open_or_truncate_local_file(
        &self,
        url: &url::Url,
        hasher_kind: HasherKind,
    ) -> Result<DownloadFile> {
        let download_dir = self.get_or_create_download_dir()?;
        let p = download_dir.join(self.get_file_path(url)?);
        if p.exists() {
            std::fs::remove_file(&p)?;
        }
        DownloadFile::new(p, hasher_kind)
    }

    pub fn get_name(&self) -> String {
        let name_path = self.path.file_name().unwrap();
        name_path.to_string_lossy().to_string()
    }

    pub fn get_or_create_build_dir(&self) -> Result<PathBuf> {
        let p = self.path.join("build");
        if !p.exists() {
            DirBuilder::new().recursive(true).create(&p)?;
        }
        Ok(p)
    }

    pub fn get_or_create_prototype_dir(&self) -> Result<PathBuf> {
        let p = self.path.join("proto");
        if !p.exists() {
            DirBuilder::new().recursive(true).create(&p)?;
        }
        Ok(p)
    }

    pub fn get_or_create_manifest_dir(&self) -> Result<PathBuf> {
        let p = self.path.join("manifests");
        if !p.exists() {
            DirBuilder::new().recursive(true).create(&p)?;
        }
        Ok(p)
    }
}

#[allow(dead_code)]
pub enum HasherKind {
    Sha256,
    Sha512,
}

pub struct DownloadFile {
    path: PathBuf,
    handle: std::fs::File,
    hasher512: sha2::Sha512,
    hasher256: sha2::Sha256,
    hasher_kind: HasherKind,
}

impl DownloadFile {
    fn new<P: AsRef<Path>>(path: P, kind: HasherKind) -> Result<Self> {
        Ok(DownloadFile {
            path: path.as_ref().to_path_buf(),
            handle: std::fs::File::options()
                .read(true)
                .write(true)
                .create_new(true)
                .open(path)?,
            hasher_kind: kind,
            hasher512: sha2::Sha512::new(),
            hasher256: sha2::Sha256::new(),
        })
    }

    pub fn get_hash(&mut self) -> String {
        match self.hasher_kind {
            HasherKind::Sha256 => hex::encode(self.hasher256.clone().finalize()),
            HasherKind::Sha512 => hex::encode(self.hasher512.clone().finalize()),
        }
    }

    pub fn get_path(&self) -> PathBuf {
        self.path.clone().to_path_buf()
    }

    #[allow(dead_code)]
    pub fn exists(&self) -> bool {
        self.path.exists()
    }
}

impl Write for DownloadFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.hasher_kind {
            HasherKind::Sha256 => self.hasher256.update(buf),
            HasherKind::Sha512 => self.hasher512.update(buf),
        };
        self.handle.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.hasher_kind {
            HasherKind::Sha256 => self.hasher256.flush()?,
            HasherKind::Sha512 => self.hasher512.flush()?,
        };
        self.handle.flush()
    }
}
