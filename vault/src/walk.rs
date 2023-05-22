use std::{
    cmp::Ordering,
    collections::BTreeSet,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use walkdir::{DirEntry, WalkDir};

use crate::Error;

pub(crate) fn top_level_manifests(path: &Path) -> impl Iterator<Item = Result<PathBuf, Error>> {
    // Since it's possible for a crate file to include nested manifests at deeper levels, we only
    // want the _first_ manifest that we encounter as we walk through directories. We'll ensure
    // this by enforcing a sort order that puts manifests first, and then not recursing into
    // directories where we've already seen a manifest.
    let mut seen = PrefixSet::default();
    WalkDir::new(path)
        .sort_by(|a, b| {
            if is_manifest(a) {
                Ordering::Less
            } else if is_manifest(b) {
                Ordering::Greater
            } else {
                a.file_name().cmp(b.file_name())
            }
        })
        .into_iter()
        .filter_entry(move |entry| {
            if is_manifest(entry) {
                match manifest_parent(entry.path()) {
                    Ok(path) => {
                        seen.insert(path);
                    }
                    Err(e) => {
                        tracing::warn!(?e, "getting manifest parent");
                    }
                }

                // If there was an error above, let's err on the side of keeping the manifest.
                // (Sorry, no pun intended.)
                true
            } else if entry.file_type().is_dir() {
                // Only iterate into a directory if it does _not_ contain a manifest.
                !seen.contains(entry.path())
            } else {
                // We're not interested in anything else.
                false
            }
        })
        .filter_map(|result| match result {
            Ok(entry) if is_manifest(&entry) => Some(Ok(entry.path().to_path_buf())),
            Err(e) => Some(Err(Error::from(e))),
            // Take out the directories that are still present.
            _ => None,
        })
}

#[derive(Default)]
struct PrefixSet(BTreeSet<PathBuf>);

impl PrefixSet {
    fn contains(&self, path: &Path) -> bool {
        path.ancestors().any(|path| self.0.contains(path))
    }

    fn insert(&mut self, manifest_parent: &Path) {
        self.0.insert(manifest_parent.to_path_buf());
    }
}

fn is_manifest(entry: &DirEntry) -> bool {
    if !entry.file_type().is_file() {
        return false;
    }

    static TITLE_CASE: OnceLock<&OsStr> = OnceLock::new();
    static LOWER_CASE: OnceLock<&OsStr> = OnceLock::new();

    let title_case = TITLE_CASE.get_or_init(|| OsStr::new("Cargo.toml"));
    let lower_case = LOWER_CASE.get_or_init(|| OsStr::new("cargo.toml"));

    matches!(entry.path().file_name(), Some(name) if &name == title_case || &name == lower_case)
}

fn manifest_parent(path: &Path) -> Result<&Path, Error> {
    path.parent()
        .ok_or_else(|| Error::ManifestAncestry(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write};

    use googletest::prelude::*;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_top_level_manifests() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let a = create_manifest_at(&temp, "a/b")?;
        let b = create_manifest_at(&temp, "b")?;
        create_manifest_at(&temp, "b/c/d")?;
        let c = create_manifest_at(&temp, "c/d")?;

        let seen =
            top_level_manifests(temp.path()).collect::<std::result::Result<Vec<_>, Error>>()?;
        assert_that!(seen, unordered_elements_are![eq(a), eq(b), eq(c)]);

        Ok(())
    }

    fn create_manifest_at(base: &TempDir, path: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
        let path = base.path().join(path);
        std::fs::create_dir_all(&path)?;

        let manifest_path = path.join("Cargo.toml");
        let mut file = File::create(&manifest_path)?;
        writeln!(&mut file, "[package]")?;
        writeln!(&mut file, r#"name = "foo""#)?;
        writeln!(&mut file, r#"version = "0.0.0""#)?;

        Ok(manifest_path)
    }
}
