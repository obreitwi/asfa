use anyhow::{bail, Result};
use clap::Clap;
use log::debug;

use crate::cfg::Config;
use crate::cli::WaitingSpinner;
use crate::cmd::Command;
use crate::ssh::SshSession;

/// Verify already uploaded files.
#[derive(Clap, Debug)]
pub struct Verify {
    /// Explicit file to verify
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Verify all filenames matching regex. See https://docs.rs/regex/latest/regex/#syntax
    #[clap(long, short = 'F', value_name = "regex")]
    filter: Option<String>,

    /// Verify last
    #[clap(short = 'n', long)]
    last: Option<usize>,

    /// Indices of files to verify as returned by `list` command.
    #[clap()]
    indices: Vec<i64>,

    /// Disable confirming deletions
    #[clap(long = "no-confirm")]
    no_confirm: bool,

    /// Sort by size (useful when specifying `--last`)
    #[clap(long, short = 'S')]
    sort_size: bool,

    /// Reverse ordering (useful when specifying `--last` and `--sort-size`)
    #[clap(long, short)]
    reverse: bool,
}

impl Command for Verify {
    fn run(&self, session: &SshSession, _config: &Config) -> Result<()> {
        debug!("Verifying remote files..");

        let files: Vec<&str> = self.files.iter().map(|s| s.as_str()).collect();

        let files_to_verify = session
            .list_files()?
            .by_indices(&self.indices[..])?
            .by_filter(self.filter.as_ref().map(|s| s.as_str()))?
            .with_all_if_none()
            .sort_by_size(self.sort_size)?
            .revert(self.reverse)
            .last(self.last)
            .by_name(&files[..], session.host.prefix_length)?;

        let spinner = WaitingSpinner::new("Verifying..".to_string());
        let files: Vec<_> = files_to_verify.iter()?.map(|e| e.1).collect();
        let hashes_actual = files[..]
            .chunks(128)
            .map(|c| {
                session
                    .get_remote_hashes(c, session.host.prefix_length)
                    .map(|h| h.into_iter())
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten();

        for (file, hash_actual) in files.iter().zip(hashes_actual) {
            let hash_expected = file.parent().unwrap().to_string_lossy();

            if *hash_actual != hash_expected {
                bail!(
                    "'{}': Expected '{}', but found '{}'",
                    file.file_name().unwrap().to_string_lossy(),
                    hash_expected,
                    hash_actual
                );
            }
        }
        spinner.finish();
        Ok(())
    }
}
