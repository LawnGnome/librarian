use std::{
    ffi::OsString,
    io::ErrorKind,
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use git2::{
    build::CheckoutBuilder, BranchType, FetchOptions, RemoteCallbacks, Repository, ResetType,
};
use indicatif::{
    MultiProgress, ParallelProgressIterator, ProgressBar, ProgressIterator, ProgressStyle,
};
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use thiserror::Error;
use walkdir::WalkDir;

use self::krate::Krate;

pub mod krate;

#[derive(Clone, Debug)]
pub struct Index(Arc<PathBuf>);

impl Index {
    #[tracing::instrument(err)]
    pub fn new(path: &Path) -> Result<Self, Error> {
        match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => {
                // Ensure the repository is initialised: just because the directory exists, doesn't
                // mean it's a valid repo!
                if !matches!(std::fs::metadata(path.join(".git")), Ok(git_metadata) if git_metadata.is_dir())
                {
                    Repository::init(path)?;
                }
                Ok(Self(Arc::new(std::fs::canonicalize(path)?)))
            }
            Ok(_) => Err(Error::NotADirectory(path.into())),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                Repository::init(path)?;
                Ok(Self(Arc::new(std::fs::canonicalize(path)?)))
            }
            Err(e) => Err(e.into()),
        }
    }

    #[tracing::instrument]
    pub fn all(&self) -> impl Iterator<Item = Result<Krate, Error>> + '_ {
        let progress = ProgressBar::new(0).with_style(
            ProgressStyle::with_template("Discovering crates: {pos}").expect("bar template"),
        );
        let names: Vec<Result<String, Error>> = WalkDir::new(self.0.as_path())
            .min_depth(1)
            .into_iter()
            .filter_entry(|entry| {
                entry
                    .file_name()
                    .as_bytes()
                    .iter()
                    .all(|c| c.is_ascii_alphanumeric() || *c == b'-' || *c == b'_')
            })
            .progress_with(progress)
            .filter_map(|result| match result {
                Ok(entry) if entry.file_type().is_dir() => None,
                Ok(entry) => {
                    let file_name = entry.file_name();
                    match file_name.to_str() {
                        Some(name) => Some(Ok(name.to_string())),
                        None => Some(Err(Error::InvalidCrateName(file_name.to_os_string()))),
                    }
                }
                Err(e) => Some(Err(Error::from(e))),
            })
            .collect();

        let crates: Vec<_> = names
            .into_par_iter()
            .progress_with_style(
                ProgressStyle::with_template("Parsing indices {wide_bar} {pos}/{len} ETA: {eta}")
                    .expect("bar template"),
            )
            .map(|result| result.and_then(|name| self.get(&name)))
            .collect();

        crates.into_iter()
    }

    #[tracing::instrument(err)]
    pub fn get(&self, name: &str) -> Result<Krate, Error> {
        let path = match name.len() {
            0 => {
                return Err(Error::EmptyCrateName);
            }
            1 => self.0.join("1"),
            2 => self.0.join("2"),
            3 => self.0.join("3").join(&name[0..1]),
            _ => self.0.join(&name[0..2]).join(&name[2..4]),
        }
        .join(name);

        Krate::open(name, &path).map_err(|e| {
            if let Error::Io(e) = &e {
                if e.kind() == ErrorKind::NotFound {
                    return Error::NotFound(name.to_string());
                }
            }
            e
        })
    }

    #[tracing::instrument(err)]
    pub fn update(&mut self, remote: &str, branch: &str) -> Result<(), Error> {
        let repo = Repository::open(self.0.as_path())?;

        Self::fetch(&repo, remote, branch)?;
        Self::checkout(&repo, branch)
    }

    #[tracing::instrument(skip(repo), err)]
    fn fetch(repo: &Repository, remote_url: &str, branch: &str) -> Result<(), Error> {
        let progress = FetchProgress::new();

        let mut remote = match repo.find_remote("origin") {
            Ok(remote) => {
                repo.remote_set_url("origin", remote_url)?;
                remote
            }
            Err(_e) => repo.remote("origin", remote_url)?,
        };

        remote.fetch(
            &[&branch],
            Some(FetchOptions::new().remote_callbacks(progress.create_callbacks())),
            None,
        )?;

        Ok(())
    }

    #[tracing::instrument(skip(repo), err)]
    fn checkout(repo: &Repository, branch: &str) -> Result<(), Error> {
        let branch = repo.find_branch(&format!("origin/{branch}"), BranchType::Remote)?;
        let tree = branch.get().peel_to_commit()?;

        let progress = ProgressBar::new(0).with_style(
            ProgressStyle::with_template(
                "Checking out files {wide_bar} {pos}/{len} ETA: {eta:>10}",
            )
            .expect("checkout progress"),
        );

        let mut options = CheckoutBuilder::new();
        options.progress(|_path, completed, total| {
            progress.set_length(total as u64);
            progress.set_position(completed as u64);
        });

        repo.reset(&tree.into_object(), ResetType::Hard, Some(&mut options))?;

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid crate name: cannot be empty")]
    EmptyCrateName,

    #[error("git2 error: {0:?}")]
    Git2(#[from] git2::Error),

    #[error("invalid crate name: {0:?}")]
    InvalidCrateName(OsString),

    #[error("io error: {0:?}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0:?}")]
    Json(#[from] serde_json::Error),

    #[error("path exists, but is not a directory: {0:?}")]
    NotADirectory(PathBuf),

    #[error("crate not found: {0}")]
    NotFound(String),

    #[error("walkdir error: {0:?}")]
    WalkDir(#[from] walkdir::Error),
}

struct FetchProgress {
    multi: MultiProgress,
    objects: ProgressBar,
    deltas: ProgressBar,
    bytes: ProgressBar,
}

impl FetchProgress {
    fn new() -> Self {
        let multi = MultiProgress::new();

        let objects = ProgressBar::new(1).with_style(
            ProgressStyle::with_template("Objects {wide_bar} {pos}/{len} ETA: {eta:>10}")
                .expect("object template"),
        );

        let deltas = ProgressBar::new(1).with_style(
            ProgressStyle::with_template("Deltas  {wide_bar} {pos}/{len} ETA: {eta:>10}")
                .expect("deltas template"),
        );

        let bytes = ProgressBar::new(0).with_style(
            ProgressStyle::with_template("Bytes transferred: {bytes}").expect("object template"),
        );

        multi.add(objects.clone());
        multi.add(deltas.clone());
        multi.add(bytes.clone());

        Self {
            multi,
            objects,
            deltas,
            bytes,
        }
    }

    fn create_callbacks(&self) -> RemoteCallbacks<'_> {
        let mut cb = RemoteCallbacks::new();

        cb.sideband_progress(|msg| {
            match std::str::from_utf8(msg) {
                Ok(s) => self
                    .multi
                    .println(s.trim_matches('\r'))
                    .expect("sideband progress"),
                Err(e) => tracing::warn!(?e, ?msg, "sideband got non UTF-8 data"),
            }

            true
        });

        cb.transfer_progress(|progress| {
            self.objects.set_length(progress.total_objects() as u64);
            self.objects.set_position(progress.indexed_objects() as u64);
            self.deltas.set_length(progress.total_deltas() as u64);
            self.deltas.set_position(progress.indexed_deltas() as u64);
            self.bytes.set_position(progress.received_bytes() as u64);

            true
        });

        cb
    }
}

impl Drop for FetchProgress {
    fn drop(&mut self) {
        self.multi.clear().expect("multi clear");
    }
}
