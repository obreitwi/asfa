use anyhow::{bail, Context, Result};
use chrono::{Local, TimeZone};
use clap::Clap;
use console::Style;
use regex::Regex;
use ssh2::FileStat;
use std::path::PathBuf;

use crate::cfg::Config;
use crate::cli::draw_boxed;
use crate::cli::{color, text};
use crate::cmd::Command;
use crate::ssh::{FileListing, SshSession};

/// List uploaded files and their URLs.
#[derive(Clap, Debug)]
pub struct List {
    /// Only list newest `n` entries. Note that entries are selected prior to sorting. That means
    /// that if you want to get the largest files by size you should not specify `--last`.
    /// Otherwise, only the last `<n>` files will be sorted by size.
    #[clap(short = 'n', long)]
    last: Option<usize>,

    /// Specify indices of files to list (if none given, list all).
    #[clap()]
    indices: Vec<i64>,

    /// Only print indices of files.
    /// This is useful to supply as input to the clean command for instance:
    /// Example: `asfa clean $(asfa list -iF "\.png$")` deletes all png.
    #[clap(long = "indices", short = 'i', conflicts_with = "url-only")]
    print_indices: bool,

    /// Show no full urls but rather filenames only. Makes for more concise output.
    #[clap(long, short)]
    filenames: bool,

    /// Filter filenames by regex. See https://docs.rs/regex/latest/regex/#syntax
    #[clap(long, short = 'F', value_name = "regex")]
    filter: Option<String>,

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

        let mut to_list: FileListing = if self.indices.len() == 0 {
            let files = session.list_files()?;
            let num_files = files.len();
            if let Some(n) = self.last {
                FileListing {
                    files: files
                        .into_iter()
                        .enumerate()
                        .skip(num_files as usize - n)
                        .collect(),
                    num_files,
                }
            } else {
                FileListing {
                    files: files.into_iter().enumerate().collect(),
                    num_files,
                }
            }
        } else {
            session.get_files_by(&self.indices, &[], session.host.prefix_length)?
        };

        if self.filter.is_some() {
            let re = Regex::new(&self.filter.as_ref().unwrap())?;
            to_list.files = to_list
                .files
                .into_iter()
                .filter(|(_, path)| {
                    re.is_match(&path.file_name().unwrap().to_string_lossy().to_string())
                })
                .collect()
        }

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
                    .files
                    .iter()
                    .map(|f| f.0)
                    .min()
                    .with_context(|| "No files to list.")
                    .unwrap();
            while num > 0 {
                num /= 10;
                num_digits += 1;
            }
            num_digits + 1 /* minus sign */
        };

        if self.url_only {
            for (_, file) in to_list.files {
                println!("{}", host.get_url(&format!("{}", file.display()))?);
            }
        } else if self.print_indices {
            for (idx, _) in to_list.files {
                print!("{} ", idx);
            }
            println!("");
        } else {
            let mut list_infos: Vec<(&(usize, PathBuf), Option<ssh2::FileStat>)> = {
                if self.stats_needed() {
                    let files = to_list.files.iter().map(|f| f.1.as_ref());
                    to_list
                        .files
                        .iter()
                        .zip(session.stat(files)?.into_iter().map(|s| Some(s)))
                        .collect()
                } else {
                    to_list.files.iter().zip(std::iter::repeat(None)).collect()
                }
            };

            if self.sort_size {
                list_infos.sort_by_key(|(_, fs)| fs.as_ref().unwrap().size.unwrap());
            }

            if self.reverse {
                list_infos.reverse();
            }

            let content: Result<Vec<String>> = list_infos
                .iter()
                .map(|((i, file), stat)| -> Result<String> {
                    Ok(format!(
                        " {idx:width$} {sep} {rev_idx:rev_width$} {sep} {size}{mtime}{url} ",
                        idx = i,
                        rev_idx = *i as i64 - to_list.num_files as i64,
                        url = if self.filenames {
                            file.file_name().unwrap().to_string_lossy().to_string()
                        } else {
                            host.get_url(&format!("{}", file.display()))?
                        },
                        width = num_digits,
                        rev_width = num_digits_rev,
                        sep = text::separator(),
                        size = if self.with_size {
                            stat.as_ref()
                                .map(|s| self.column_size(s))
                                .unwrap_or(Ok("".to_string()))?
                        } else {
                            "".to_string()
                        },
                        mtime = if self.with_time {
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
    /// Return whether or not we need to fetch stats
    fn stats_needed(&self) -> bool {
        self.with_size || self.sort_size || self.with_time
    }

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
            if size > 1000 {
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
