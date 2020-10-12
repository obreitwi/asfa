use anyhow::{bail, Context, Result};
use clap::Clap;
use console::Style;
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
    /// Only list the remote URLs (useful for copying and scripting).
    #[clap(short, long = "url-only")]
    url_only: bool,

    /// Only list last `n` entries
    #[clap(short = 'n', long)]
    last: Option<usize>,

    /// Print file sizes
    #[clap(long, short = 's')]
    with_size: bool,

    /// Specify indices of files to list (if none given, list all).
    #[clap()]
    indices: Vec<i64>,
}

impl Command for List {
    fn run(&self, session: &SshSession, _config: &Config) -> Result<()> {
        let host = &session.host;

        let to_list: FileListing = if self.indices.len() == 0 {
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

        let num_digits = {
            let mut num_digits = 0;
            let mut num = to_list.num_files;
            while num > 0 {
                num /= 10;
                num_digits += 1;
            }
            num_digits
        };

        if self.url_only {
            for (_, file) in to_list.files {
                println!("{}", host.get_url(&format!("{}", file.display()))?);
            }
        } else {
            let list_infos: Vec<(&(usize, PathBuf), Option<ssh2::FileStat>)> = {
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
            let content: Result<Vec<String>> = list_infos
                .iter()
                .map(|((i, file), stat)| -> Result<String> {
                    Ok(format!(
                        "{idx:width$}{sep}{rev_idx:rev_width$}{sep}{size} {url} ",
                        idx = i,
                        rev_idx = *i as i64 - to_list.num_files as i64,
                        url = host.get_url(&format!("{}", file.display()))?,
                        width = num_digits,
                        rev_width = num_digits + 1,
                        sep = text::separator(),
                        size = if self.with_size {
                            stat.as_ref()
                                .map(|s| self.size_column(s))
                                .unwrap_or(Ok("".to_string()))?
                        } else {
                            "".to_string()
                        }
                    ))
                })
                .collect();
            draw_boxed(
                format!(
                    "{listing} remote files",
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
        self.with_size
    }

    fn size_column(&self, stat: &FileStat) -> Result<String> {
        let possible = ["", "K", "M", "G", "T", "P", "E"];
        let mut size: u64 = stat.size.with_context(|| "No file size defined!")?;
        for (i, s) in possible.iter().enumerate() {
            if size > 1000 {
                size = size >> 10;
                continue;
            } else {
                return Ok(format!(
                    "{:>6.2}{}{}",
                    stat.size.unwrap() as f64 / (1 << (i * 10)) as f64,
                    s,
                    text::separator()
                ));
            }
        }
        bail!("Invalid size argument provided.")
    }
}
