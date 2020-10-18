use anyhow::{bail, Result};
use clap::Clap;
use log::debug;

use crate::cfg::Config;
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
            .sort_by_size(self.sort_size)?
            .revert(self.reverse)
            .last(self.last)
            .by_name(&files[..], session.host.prefix_length)?;

        let no_files_selected = files_to_verify.iter()?.count() == 0;
        let files_to_verify = files_to_verify.add_all(no_files_selected);

        for (_, file, _) in files_to_verify.iter()? {
            let hash_expected = file.parent().unwrap().to_string_lossy();
            let hash_actual = session.get_remote_hash(file, hash_expected.len() as u8)?;

            if hash_actual != hash_expected {
                bail!(
                    "{}: Expected '{}', but found '{}'",
                    file.display(),
                    hash_expected,
                    hash_actual
                );
            }
        }
        Ok(())
    }
}
