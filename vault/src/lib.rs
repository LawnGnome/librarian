use std::{
    ops::Deref,
    path::{Path, PathBuf},
};

mod error;
mod manifest;
mod walk;

pub use error::Error;
use manifest::Manifest;

#[derive(Debug)]
pub struct Vault(PathBuf);

impl Vault {
    pub fn new<T>(path: T) -> Self
    where
        T: ToOwned<Owned = PathBuf>,
    {
        Self(path.to_owned())
    }

    pub fn iter_crate_versions(&self) -> impl Iterator<Item = Result<CrateVersion, Error>> + '_ {
        walk::top_level_manifests(&self.0).map(|result| {
            result.and_then(|path| -> Result<CrateVersion, Error> {
                let manifest = Manifest::parse_file(&path)?;

                Ok(CrateVersion {
                    crate_name: manifest.crate_name().to_string(),
                    version: manifest.crate_version().to_string(),
                    path,
                })
            })
        })
    }

    pub fn crate_version_path(&self, crate_name: &str, version: &str) -> Result<PathBuf, Error> {
        let mut path = self.0.join(
            crate_name
                .get(0..1)
                .ok_or_else(|| Error::InvalidCrateName(crate_name.to_string()))?,
        );

        if let Some(two) = crate_name.get(0..2) {
            path = path.join(two);
        }

        path = path.join(crate_name);

        if version.is_empty() {
            Err(Error::InvalidCrateVersion(version.to_string()))
        } else {
            Ok(path.join(version))
        }
    }
}

impl AsRef<Path> for Vault {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Deref for Vault {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.0.as_path()
    }
}

#[derive(Debug, Clone)]
pub struct CrateVersion {
    pub crate_name: String,
    pub version: String,
    pub path: PathBuf,
}
