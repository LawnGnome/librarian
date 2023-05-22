use std::{fs::File, io::Read, path::Path};

use serde::Deserialize;

use crate::Error;

#[derive(Deserialize)]
pub(crate) struct Manifest {
    package: Package,
}

impl Manifest {
    pub(crate) fn parse_file(path: &Path) -> Result<Self, Error> {
        let mut file = File::open(path).map_err(|e| Error::ManifestOpen(path.to_path_buf(), e))?;
        let mut s = String::new();

        file.read_to_string(&mut s)
            .map_err(|e| Error::ManifestRead(path.to_path_buf(), e))?;
        toml::from_str(&s).map_err(|e| Error::ManifestParse(path.to_path_buf(), e))
    }

    pub(crate) fn crate_name(&self) -> &str {
        &self.package.name
    }

    pub(crate) fn crate_version(&self) -> &str {
        &self.package.version
    }
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
}
