use anyhow::Result;
use clap::Clap;
use log::info;
use std::path::PathBuf;

use crate::cfg::Config;
use crate::cmd::Command;
use crate::ssh::SshSession;

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

        let files = session.list_files()?;
        let num_files = files.len() as i64;

        let to_list: Vec<(usize, &PathBuf)> = if self.indices.len() == 0 {
            if let Some(n) = self.last {
                files
                    .iter()
                    .enumerate()
                    .skip(num_files as usize - n)
                    .collect()
            } else {
                files.iter().enumerate().collect()
            }
        } else {
            let mut selected = Vec::with_capacity(self.indices.len());
            for i in &self.indices {
                let idx = if *i < 0 { num_files + i } else { *i } as usize;
                selected.push((idx, &files[idx]));
            }
            selected
        };

        let num_digits = {
            let mut num_digits = 0;
            let mut num = num_files;
            while num > 0 {
                num /= 10;
                num_digits += 1;
            }
            num_digits
        };

        if self.url_only {
            for (_, file) in to_list {
                println!("{}", host.get_url(&format!("{}", file.display()))?);
            }
        } else {
            for (i, file) in to_list.iter() {
                println!(
                    "[{idx:width$}|{rev_idx:rev_width$}] {url}",
                    idx = i,
                    rev_idx = *i as i64 - num_files,
                    url = host.get_url(&format!("{}", file.display()))?,
                    width = num_digits,
                    rev_width = num_digits + 1
                );
            }
        }
        Ok(())
    }
}
