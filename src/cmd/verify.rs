use anyhow::{bail, Result};
use clap::Clap;
use log::debug;

use crate::cfg::Config;
use crate::cli::{color, WaitingSpinner};
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

    /// Select files newer than the given duration. Durations can be:seconds (sec, s), minutes
    /// (min, m), days (d), weeks (w), months (M) or years (y).
    #[clap(long = "newer")]
    select_newer: Option<String>,

    /// Select files older than the given duration. Durations can be:seconds (sec, s), minutes
    /// (min, m), days (d), weeks (w), months (M) or years (y).
    #[clap(long = "older")]
    select_older: Option<String>,

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
            .with_all_if_none(true)
            .select_newer(self.select_newer.as_deref())?
            .select_older(self.select_older.as_deref())?
            .sort_by_size(self.sort_size)?
            .revert(self.reverse)
            .last(self.last)
            .by_name(&files[..], session.host.prefix_length)?;

        let message = "Verifying...";
        let files: Vec<_> = files_to_verify.iter()?.map(|e| e.1).collect();

        let num_files = files.len();
        if num_files == 0 {
            bail!("No files to verify..");
        }

        let spinner = WaitingSpinner::new(format!("{} 0/{}", message, &num_files));
        let filename_max = files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().chars().count())
            .max()
            .unwrap()
            + 1;

        let chunk_size = 16;
        let hashes_actual = files[..]
            .chunks(chunk_size)
            .map(|c| session.get_remote_hashes(c, session.host.prefix_length));

        let mut failure = Vec::new();
        for (idx, (files, hashes_actual)) in
            files[..].chunks(chunk_size).zip(hashes_actual).enumerate()
        {
            spinner.set_message(format!("{} {}/{}", message, idx * chunk_size, &num_files))?;
            let hashes_actual = hashes_actual?;
            for (file, hash_actual) in files.iter().zip(hashes_actual) {
                let hash_expected = file.parent().unwrap().to_string_lossy();
                let filename = file.file_name().unwrap().to_string_lossy();
                let filename_len = filename.chars().count();
                let separator_len = filename_max - filename_len;
                if hash_actual != hash_expected {
                    let msg = format!(
                        "{} {} {} Expected: {} Found: {}",
                        color::failure.apply_to("✗"),
                        color::filename.apply_to(&filename),
                        ".".repeat(separator_len),
                        color::success.apply_to(hash_expected),
                        color::failure.apply_to(hash_actual),
                    );
                    spinner.println(msg)?;
                    failure.push(file);
                } else {
                    spinner.println(format!(
                        "{} {} {} {}.",
                        color::success.apply_to("✓"),
                        color::filename.apply_to(file.file_name().unwrap().to_string_lossy()),
                        ".".repeat(separator_len),
                        color::success.apply_to("Verified"),
                    ))?;
                }
            }
        }
        spinner.set_message("Verifying.. done".to_string())?;
        spinner.finish();

        if failure.len() > 0 {
            bail!("{} files failed to verify.", failure.len());
        } else {
            Ok(())
        }
    }
}
