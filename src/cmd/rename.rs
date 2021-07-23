use anyhow::Result;
use atty::Stream;
use clap::Clap;
use console::Style;

use crate::cfg::Config;
use crate::cli::color;
use crate::cli::draw_boxed;
use crate::cmd::Command;
use crate::ssh::SshSession;

/// Rename an already uploaded file
#[derive(Clap, Debug)]
pub struct Rename {
    /// Show all details, can be set globally in config file.
    #[clap(long, short)]
    details: bool,

    /// Specify index of file to rename
    #[clap()]
    index: Option<i64>,

    /// Input filename
    #[clap(short='f', long="filename")]
    input_filename: Option<String>,

    /// New name to rename file
    #[clap()]
    new_filename: String,

    /// If `details` is set to true in config, --no-details can be specified to suppress output.
    #[clap(long, short = 'D')]
    no_details: bool,
}

impl Command for List {
    fn run(&self, session: &SshSession, config: &Config) -> Result<()> {
        let host = &session.host;

        let show_details = (self.details || config.details) && !self.no_details;

        if self.index.is_empty() && self.input_filename.is_empty() {
            log::error!("Please specify either a remote index or a local file to rename via --filename.");
        }

        let to_list = session
            .list_files()?
            .by_name(
                self.files.iter().map(|pb| pb.to_string_lossy()),
                session.host.prefix_length,
                /* bail_when_missing = */ false,
            )?
            .by_indices(&self.indices[..])?
            .with_stats(show_details)?;

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
