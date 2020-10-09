use anyhow::Result;
use clap::Clap;
use log::info;

use crate::cfg::Config;
use crate::cmd::Command;
use crate::ssh::{FileListing, SshSession};

/// List uploaded files and their URLs.
#[derive(Clap, Debug)]
pub struct List {
    /// Only list the remote URLs (useful for copying).
    #[clap(short, long = "url-only")]
    url_only: bool,

    /// Only list last `n` entries
    #[clap(short = 'n', long)]
    last: Option<usize>,

    /// Specify indices of files to list (if none given, list all).
    #[clap()]
    indices: Vec<i64>,
}

impl Command for List {
    fn run(&self, session: &SshSession, _config: &Config) -> Result<()> {
        info!("Listing remote files:");

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
            for (i, file) in to_list.files.iter() {
                println!(
                    "[{idx:width$}|{rev_idx:rev_width$}] {url}",
                    idx = i,
                    rev_idx = *i as i64 - to_list.num_files as i64,
                    url = host.get_url(&format!("{}", file.display()))?,
                    width = num_digits,
                    rev_width = num_digits + 1
                );
            }
        }
        Ok(())
    }
}
