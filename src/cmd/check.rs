use anyhow::{bail, Result};
use atty::Stream;
use clap::Clap;
use console::Style;
use std::path::PathBuf;

use crate::cfg::Config;
use crate::cli::{color, draw_boxed};
use crate::cmd::Command;
use crate::ssh::SshSession;

/// Check if a given local file is already present on the remote site.
#[derive(Clap, Debug)]
pub struct Check {
    /// File(s) to check for.
    #[clap()]
    files: Vec<PathBuf>,

    /// Show all details, can be set globally in config file.
    #[clap(long, short)]
    details: bool,

    /// Show no full urls but rather filenames only. Makes for more concise output.
    #[clap(long, short)]
    filenames: bool,

    /// If `details` is set to true in config, --no-details can be specified to suppress output.
    #[clap(long, short = 'D')]
    no_details: bool,

    /// Only list the remote URLs (useful for copying and scripting).
    #[clap(short, long = "url-only")]
    url_only: bool,

    /// Print remote modification time
    #[clap(long, short = 't')]
    with_time: bool,

    /// Print file sizes
    #[clap(long, short = 's')]
    with_size: bool,
}

impl Command for Check {
    fn run(&self, session: &SshSession, config: &Config) -> Result<()> {
        let show_details = (self.details || config.details) && !self.no_details;

        let found = session
            .list_files()?
            .by_name(
                self.files.iter().map(|pb| pb.to_string_lossy()),
                session.host.prefix_length,
                /* bail_when_missing = */ false,
            )?
            .with_stats(show_details || self.with_time || self.with_size)?;

        if self.url_only {
            for (_, file, _) in found.iter() {
                println!("{}", session.host.get_url(&format!("{}", file.display()))?);
            }
        } else if !config.is_silent() {
            let content = found.format_files(
                Some(&session.host),
                self.filenames,
                show_details || self.with_size,
                show_details || self.with_time,
            )?;

            // Only print fancy boxes if we are attached to a TTY -> otherwise, just dump data in
            // parseable format
            if atty::is(Stream::Stdout) {
                draw_boxed(
                    format!(
                        "{} remote files:",
                        Style::new().bold().green().bright().apply_to("Found")
                    ),
                    content.iter().map(|s| s.as_ref()),
                    &color::frame,
                )?;
            } else {
                for line in content {
                    println!("{}", line);
                }
            }
        }

        if found.iter().count() == self.files.len() {
            Ok(())
        } else {
            bail!(
                "# of file expected/found differs: {}/{}",
                self.files.len(),
                found.iter().count()
            );
        }
    }
}
