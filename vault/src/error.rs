use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid crate name: {0:?}")]
    InvalidCrateName(String),

    #[error("invalid crate version: {0:?}")]
    InvalidCrateVersion(String),

    #[error("manifest does not have a parent: {0:?}")]
    ManifestAncestry(PathBuf),

    #[error("opening manifest at {0:?}: {1:?}")]
    ManifestOpen(PathBuf, #[source] std::io::Error),

    #[error("parsing manifest at {0:?}: {1:?}")]
    ManifestParse(PathBuf, #[source] toml::de::Error),

    #[error("reading manifest at {0:?}: {1:?}")]
    ManifestRead(PathBuf, #[source] std::io::Error),

    #[error("walking vault directories: {0:?}")]
    WalkDir(#[from] walkdir::Error),
}
