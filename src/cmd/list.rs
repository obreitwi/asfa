use anyhow::{bail, Context, Result};
use chrono::{Local, TimeZone};
use clap::Clap;
use console::Style;
use ssh2::FileStat;

use crate::cfg::Config;
use crate::cli::draw_boxed;
use crate::cli::{color, text};
use crate::cmd::Command;
use crate::ssh::SshSession;

/// List uploaded files and their URLs.
#[derive(Clap, Debug)]
pub struct List {
    /// Show all details
    #[clap(long, short)]
    details: bool,

    /// Show no full urls but rather filenames only. Makes for more concise output.
    #[clap(long, short)]
    filenames: bool,

    /// Filter filenames by regex. See https://docs.rs/regex/latest/regex/#syntax
    #[clap(long, short = 'F', value_name = "regex")]
    filter: Option<String>,

    /// Specify indices of files to list (if none given, list all).
    #[clap()]
    indices: Vec<i64>,

    /// Only list newest `n` entries. Note that entries are selected prior to sorting. That means
    /// that if you want to get the largest files by size you should not specify `--last`.
    /// Otherwise, only the last `<n>` files will be sorted by size.
    #[clap(short = 'n', long)]
    last: Option<usize>,

    /// Only print indices of files.
    /// This is useful to supply as input to the clean command for instance:
    /// Example: `asfa clean $(asfa list -iF "\.png$")` deletes all png.
    #[clap(long = "indices", short = 'i', conflicts_with = "url-only")]
    print_indices: bool,

    /// Reverse listing.
    #[clap(long, short)]
    reverse: bool,

    /// Sort listing by size
    #[clap(long, short = 'S')]
    sort_size: bool,

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
    fn run(&self, session: &SshSession, _config: &Config) -> Result<()> {
        let host = &session.host;

        let to_list = session
            .list_files()?
            .by_indices(&self.indices[..])?
            .by_filter(self.filter.as_ref().map(|f| f.as_str()))?
            .with_all_if_none()
            .last(self.last)
            .sort_by_size(self.sort_size)?
            .revert(self.reverse)
            .with_stats(self.details || self.with_time || self.with_size)?;

        let num_digits = {
            let mut num_digits = 0;
            let mut num = to_list.num_files;
            while num > 0 {
                num /= 10;
                num_digits += 1;
            }
            num_digits
        };

        // reverse digits
        let num_digits_rev = {
            let mut num_digits = 0;
            let mut num = to_list.num_files
                - to_list
                    .iter()?
                    .map(|f| f.0)
                    .min()
                    .with_context(|| "No files to list.")
                    .unwrap_or(0);
            while num > 0 {
                num /= 10;
                num_digits += 1;
            }
            num_digits + 1 /* minus sign */
        };

        if self.url_only {
            for (_, file, _) in to_list.iter()? {
                println!("{}", host.get_url(&format!("{}", file.display()))?);
            }
        } else if self.print_indices {
            for idx in to_list.indices {
                print!("{} ", idx);
            }
            println!("");
        } else {
            let content: Result<Vec<String>> = to_list
                .iter()?
                .map(|(i, file, stat)| -> Result<String> {
                    Ok(format!(
                        " {idx:width$} {sep} {rev_idx:rev_width$} {sep} {size}{mtime}{url} ",
                        idx = i,
                        rev_idx = i as i64 - to_list.num_files as i64,
                        url = if self.filenames {
                            file.file_name().unwrap().to_string_lossy().to_string()
                        } else {
                            host.get_url(&format!("{}", file.display()))?
                        },
                        width = num_digits,
                        rev_width = num_digits_rev,
                        sep = text::separator(),
                        size = if self.details || self.with_size {
                            stat.as_ref()
                                .map(|s| self.column_size(s))
                                .unwrap_or(Ok("".to_string()))?
                        } else {
                            "".to_string()
                        },
                        mtime = if self.details || self.with_time {
                            stat.as_ref()
                                .map(|s| self.column_time(s))
                                .unwrap_or(Ok("".to_string()))?
                        } else {
                            "".to_string()
                        }
                    ))
                })
                .collect();
            draw_boxed(
                format!(
                    "{listing} remote files:",
                    listing = Style::new().bold().green().bright().apply_to("Listing")
                ),
                content?.iter().map(|s| s.as_ref()),
                &color::frame,
            )?;
        }
        Ok(())
    }
}

impl List {
    fn column_time(&self, stat: &FileStat) -> Result<String> {
        let mtime = Local.timestamp(stat.mtime.with_context(|| "File has no mtime.")? as i64, 0);
        Ok(format!(
            "{mtime} {sep} ",
            mtime = mtime.format("%Y-%m-%d %H:%M:%S").to_string(),
            sep = text::separator()
        ))
    }

    fn column_size(&self, stat: &FileStat) -> Result<String> {
        let possible = ["", "K", "M", "G", "T", "P", "E"];
        let mut size: u64 = stat.size.with_context(|| "No file size defined!")?;
        for (i, s) in possible.iter().enumerate() {
            if size >= 1000 {
                size = size >> 10;
                continue;
            } else {
                return Ok(format!(
                    "{size:>6.2}{suffix} {sep} ",
                    size = stat.size.unwrap() as f64 / (1 << (i * 10)) as f64,
                    suffix = s,
                    sep = text::separator()
                ));
            }
        }
        bail!("Invalid size argument provided.")
    }
}
