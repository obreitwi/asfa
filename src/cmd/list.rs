use anyhow::Result;
use atty::Stream;
use clap::{Clap,AppSettings};
use console::Style;

use crate::cfg::Config;
use crate::cli::color;
use crate::cli::draw_boxed;
use crate::cmd::Command;
use crate::ssh::SshSession;

/// List uploaded files and their URLs.
#[derive(Clap, Debug)]
#[clap(global_setting=AppSettings::AllowNegativeNumbers)]
pub struct List {
    /// Show all details, can be set globally in config file.
    #[clap(long, short)]
    details: bool,

    /// Show no full urls but rather filenames only. Makes for more concise output.
    #[clap(long, short)]
    filenames: bool,

    /// Filter filenames by regex. See <https://docs.rs/regex/latest/regex/#syntax>
    #[clap(long, short = 'F', value_name = "regex")]
    filter: Option<String>,

    /// Only list first `N` entries.
    #[clap(short = 'N', long, conflicts_with="last")]
    first: Option<usize>,

    /// Specify indices of files to list (if none given, list all).
    #[clap()]
    indices: Vec<i64>,

    /// Only list last `n` entries.
    #[clap(short = 'n', long, conflicts_with="first")]
    last: Option<usize>,

    /// If `details` is set to true in config, --no-details can be specified to suppress output.
    #[clap(long, short = 'D')]
    no_details: bool,

    /// Only print indices of files.
    /// This is useful to supply as input to the clean command for instance:
    /// Example: `asfa clean $(asfa list -iF "\.png$")` deletes all png.
    #[clap(long = "indices", short = 'i', conflicts_with = "url-only")]
    print_indices: bool,

    /// Reverse listing.
    #[clap(long, short)]
    reverse: bool,

    /// Select files newer than the given duration. Durations can be:seconds (sec, s), minutes
    /// (min, m), days (d), weeks (w), months (M) or years (y).
    #[clap(long = "newer")]
    select_newer: Option<String>,

    /// Select files older than the given duration. Durations can be:seconds (sec, s), minutes
    /// (min, m), days (d), weeks (w), months (M) or years (y).
    #[clap(long = "older")]
    select_older: Option<String>,

    /// Sort listing by size
    #[clap(long, short = 'S')]
    sort_size: bool,

    /// Sort listing by modification time (useful when using `--filter` and `--last`).
    #[clap(long, short = 'T')]
    sort_time: bool,

    /// Only list the remote URLs (useful for copying and scripting).
    #[clap(short, long = "url-only", conflicts_with = "indices")]
    url_only: bool,

    /// Print remote modification time
    #[clap(long, short = 't')]
    with_time: bool,

    /// Print file sizes
    #[clap(long, short = 's')]
    with_size: bool,
}

impl Command for List {
    fn run(&self, session: &SshSession, config: &Config) -> Result<()> {
        let host = &session.host;

        let show_details = (self.details || config.details) && !self.no_details;

        let to_list = session
            .list_files()?
            .by_indices(&self.indices[..])?
            .by_filter(self.filter.as_deref())?
            .with_all_if_none(self.filter.is_none())
            .select_newer(self.select_newer.as_deref())?
            .select_older(self.select_older.as_deref())?
            .sort_by_size(self.sort_size)?
            .sort_by_time(self.sort_time)?
            .first(self.first)
            .last(self.last)
            .revert(self.reverse)
            .with_stats(show_details || self.with_time || self.with_size)?;

        if !config.is_silent() {
            if self.url_only {
                for (_, file, _) in to_list.iter() {
                    println!("{}", host.get_url(&format!("{}", file.display()))?);
                }
            } else if self.print_indices {
                for idx in to_list.indices {
                    print!("{} ", idx);
                }
                println!();
            } else {
                let content = to_list.format_files(
                    Some(&session.host),
                    self.filenames,
                    show_details || self.with_size,
                    show_details || self.with_time,
                )?;

                let content = if content.is_empty() {
                    vec![format!(
                        "{}(There are no remote files to show.)",
                        if atty::is(Stream::Stdout) { " " } else { "" }
                    )]
                } else {
                    content
                };

                // Only print fancy boxes if we are attached to a TTY -> otherwise, just dump data in
                // parseable format
                if atty::is(Stream::Stdout) {
                    draw_boxed(
                        format!(
                            "{listing} remote files:",
                            listing = Style::new().bold().green().bright().apply_to("Listing")
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
        }
        Ok(())
    }
}
