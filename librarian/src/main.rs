use std::{collections::HashSet, path::PathBuf, str::FromStr};

use clap::{Parser, Subcommand};
use corpus::Corpus;
use index::{krate::Krate, Index};
use indicatif::{ParallelProgressIterator, ProgressStyle};
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

mod corpus;
mod index;

#[derive(Parser)]
struct Opt {
    /// Path to the crates.io index repo.
    #[arg(short, long)]
    index: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Update the index repo.
    IndexUpdate {
        /// Index repo branch to check out.
        #[arg(long, default_value = "master")]
        branch: String,

        /// Index repo remote.
        #[arg(long, default_value = "https://github.com/rust-lang/crates.io-index")]
        remote: String,
    },
    /// Populate crates from the index by downloading them from static.crates.io and extracting
    /// them locally.
    ///
    /// Unless `--crates` is provided, all crates in the index will be downloaded.
    Populate {
        /// Path to place the extracted crates in.
        #[arg(short, long)]
        corpus: PathBuf,

        /// If given, only these (comma separated) crates will be downloaded.
        #[arg(long)]
        crates: Option<CrateSet>,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let opt = Opt::parse();
    let mut index = Index::new(&opt.index)?;

    match opt.command {
        Command::IndexUpdate { branch, remote } => index.update(&remote, &branch)?,
        Command::Populate { corpus, crates } => {
            let corpus = Corpus::new(corpus)?;

            let crates: Vec<Krate> = match crates {
                Some(crates) => crates
                    .0
                    .into_par_iter()
                    .map(|name| index.get(&name))
                    .collect::<Result<_, _>>()?,
                None => index.all().collect::<Result<_, index::Error>>()?,
            };

            let versions = crates
                .into_par_iter()
                .progress_with_style(ProgressStyle::with_template(
                    "Hydrating crate versions {wide_bar} {pos}/{len} ETA: {eta}",
                )?)
                .map(|krate| {
                    krate
                        .iter_versions()
                        .map(|(num, version)| (version.name().to_string(), num.clone()))
                        .collect::<Vec<(String, String)>>()
                })
                .flatten()
                .collect::<Vec<_>>();

            versions
                .into_par_iter()
                .progress_with_style(ProgressStyle::with_template(
                    "Downloading crates {wide_bar} {pos}/{len} ETA: {eta}",
                )?)
                .try_for_each(|(name, num)| match corpus.populate(&name, &num) {
                    Ok(_path) => Ok(()),
                    Err(e) => {
                        tracing::error!(?name, ?num, ?e, "error populating version");
                        std::fs::remove_dir_all(corpus.path(&name, &num)?)
                            .map_err(corpus::Error::from)
                    }
                })?;
        }
    }

    Ok(())
}

#[derive(Clone)]
struct CrateSet(HashSet<String>);

impl FromStr for CrateSet {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            s.split(',').map(str::trim).map(String::from).collect(),
        ))
    }
}
