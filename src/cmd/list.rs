use anyhow::Result;
use clap::Clap;
use log::info;

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
    #[clap(short = "n", long)]
    last: Option<usize>,

    /// Specify indices of files to list (if none given, list all).
    #[clap()]
    indices: Vec<i64>,
}

impl Command for List {
    fn run(&self, session: &SshSession, _config: &Config) -> Result<()> {
        info!("Listing remote files:");

        let host = &session.host;

        let files = session.list_files(&host.folder)?;
        let num_files = files.len() as i64;

        let to_list: Vec<(usize, &String)> = if self.indices.len() == 0 {
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

        if self.url_only {
            for (_, fd) in to_list {
                println!("{}", host.get_url(fd)?);
            }
        } else {
            for (i, file) in to_list.iter() {
                println!("[{}|{}] {}", i, *i as i64 - num_files, host.get_url(file)?);
            }
        }

        Ok(())
    }
}
