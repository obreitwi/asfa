use anyhow::{bail, Context, Result};
use clap::{Clap,AppSettings};
use dialoguer::{theme::ColorfulTheme, Confirm};
use log::debug;
use std::path::Path;

use crate::cfg::Config;
use crate::cli::color;
use crate::cmd::Command;
use crate::file_listing::FileListing;
use crate::ssh::SshSession;

/// Clear already uploaded files.
#[derive(Clap, Debug)]
#[clap(global_setting=AppSettings::AllowNegativeNumbers)]
pub struct Clean {
    /// Clean all remote files (dangerous!)
    #[clap(long)]
    all: bool,

    /// Show all details in confirmation, can be set globally in config file.
    #[clap(long, short)]
    details: bool,

    /// Explicit file to delete
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Filter filenames by regex. See https://docs.rs/regex/latest/regex/#syntax
    #[clap(long, short = 'F', value_name = "regex")]
    filter: Option<String>,

    /// Delete last
    #[clap(short = 'n', long)]
    last: Option<usize>,

    /// Indices of files to delete as returned by `list` command.
    #[clap()]
    indices: Vec<i64>,

    /// Disable confirming deletions
    #[clap(long = "no-confirm")]
    no_confirm: bool,

    /// If `details` is set to true in config, --no-details can be specified to suppress output.
    #[clap(long, short = 'D')]
    no_details: bool,

    /// Select files newer than the given duration. Durations can be:seconds (sec, s), minutes
    /// (min, m), days (d), weeks (w), months (M) or years (y).
    #[clap(long = "newer")]
    select_newer: Option<String>,

    /// Select files older than the given duration. Durations can be:seconds (sec, s), minutes
    /// (min, m), days (d), weeks (w), months (M) or years (y).
    #[clap(long = "older")]
    select_older: Option<String>,

    /// Sort by size (useful when specifying `--filter`/`--last`)
    #[clap(long, short = 'S')]
    sort_size: bool,

    /// Sort by modification time (useful when using `--filter` and `--last`).
    #[clap(long, short = 'T')]
    sort_time: bool,

    /// Reverse ordering (useful when specifying `--last` and `--sort-{size,time}`)
    #[clap(long, short)]
    reverse: bool,
}

impl Command for Clean {
    fn run(&self, session: &SshSession, config: &Config) -> Result<()> {
        debug!("Cleaning remote files..");

        let files: Vec<&str> = self.files.iter().map(|s| s.as_str()).collect();

        let show_details = (self.details || config.details) && !self.no_details;

        let files_to_delete = session
            .list_files()?
            .with_all(self.all)
            .by_indices(&self.indices[..])?
            .by_filter(self.filter.as_deref())?
            .with_all_if_none(self.select_newer.is_some() || self.select_older.is_some())
            .select_newer(self.select_newer.as_deref())?
            .select_older(self.select_older.as_deref())?
            .sort_by_size(self.sort_size)?
            .sort_by_time(self.sort_time)?
            .revert(self.reverse)
            .last(self.last)
            .by_name(
                files.iter(),
                session.host.prefix_length,
                /* bail_when_missing = */ true,
            )?
            .with_stats(show_details && !self.no_confirm)?;

        let do_delete = self.no_confirm || self.user_confirm_deletion(&files_to_delete)?;

        let remove_file =
            |file_to_delete: &Path| -> Result<()> {
                if file_to_delete.components().count() != 2 {
                    bail!("Invalid filename: {}", file_to_delete.display());
                }

                session.remove_folder(file_to_delete.parent().with_context(|| {
                    format!("File had not parent: {}", file_to_delete.display())
                })?)?;
                Ok(())
            };

        if do_delete {
            for (_, file, _) in files_to_delete.iter() {
                remove_file(&file)?
            }
        }

        Ok(())
    }
}

impl Clean {
    /// Have the user confirm deletions
    fn user_confirm_deletion(&self, files: &FileListing) -> Result<bool> {
        let with_stats = files.has_stats();
        // If we have stats, print only the filename to shorten the line
        let formatted_files = files.format_files(None, with_stats, with_stats, with_stats)?;

        crate::cli::draw_boxed(
            &format!(
                "Will {delete} the following files:",
                delete = console::Style::new()
                    .bold()
                    .red()
                    .bright()
                    .apply_to("delete")
            )
            .as_str(),
            formatted_files.iter().map(|s| s.as_str()),
            &color::frame,
        )?;
        Ok(Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Delete files?")
            .default(false)
            .interact()?)
    }
}
