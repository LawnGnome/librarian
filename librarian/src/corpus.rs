use std::{io::ErrorKind, path::PathBuf};

use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use tar::Archive;
use tempfile::tempdir_in;
use thiserror::Error;
use vault::Vault;

#[derive(Debug)]
pub struct Corpus {
    client: Client,
    vault: Vault,
}

impl Corpus {
    #[tracing::instrument(err)]
    pub fn new(path: PathBuf) -> Result<Self, Error> {
        std::fs::create_dir_all(&path)?;

        Ok(Self {
            client: Client::new(),
            vault: Vault::new(path),
        })
    }

    pub fn path(&self, krate: &str, num: &str) -> Result<PathBuf, Error> {
        Ok(self.vault.crate_version_path(krate, num)?)
    }

    #[tracing::instrument(err)]
    pub fn populate(&self, name: &str, num: &str) -> Result<PathBuf, Error> {
        let temp = tempdir_in(&self.vault)?;

        let path = self.path(name, num)?;
        let path = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => {
                return Ok(path);
            }
            Ok(_metadata) => {
                return Err(Error::NotADirectory(path));
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                std::fs::create_dir_all(&path)?;
                std::fs::canonicalize(path)?
            }
            Err(e) => {
                return Err(e.into());
            }
        };

        let resp = self
            .client
            .get(format!(
                "https://static.crates.io/crates/{name}/{name}-{num}.crate"
            ))
            .send()?;

        let mut zr = GzDecoder::new(resp);
        let mut archive = Archive::new(&mut zr);
        archive.set_overwrite(true);
        archive.unpack(&temp)?;

        std::fs::rename(temp.path().join(format!("{name}-{num}")), &path)?;
        Ok(path)
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("io error: {0:?}")]
    Io(#[from] std::io::Error),

    #[error("path exists, but is not a directory: {0:?}")]
    NotADirectory(PathBuf),

    #[error("reqwest error: {0:?}")]
    Reqwest(#[from] reqwest::Error),

    #[error("vault error: {0:?}")]
    Vault(#[from] vault::Error),
}
