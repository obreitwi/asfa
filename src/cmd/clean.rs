use anyhow::{bail, Result};
use clap::Clap;
use log::debug;
use std::path::{Path, PathBuf};

use crate::cfg::Config;
use crate::cmd::Command;
use crate::ssh::SshSession;
use crate::util::get_hash;

/// Clear already uploaded files.
#[derive(Clap, Debug)]
pub struct Clean {
    /// Clean all remote files (dangerous!)
    #[clap(long = "all")]
    all: bool,

    /// Excplicit file to delete
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Indices of files to delete as returned by `list` command.
    #[clap()]
    indices: Vec<i64>,
}

impl Command for Clean {
    fn run(&self, session: &SshSession, _config: &Config) -> Result<()> {
        debug!("Cleaning remote files..");

        let files = session.list_files(&session.host.folder)?;
        let num_files = files.len() as i64;

        let remove_idx = |idx: usize| -> Result<()> {
            let file_to_delete = &files[idx];
            let file_split = file_to_delete.split("/").collect::<Vec<&str>>();

            if file_split.len() != 2 {
                bail!("Invalid filename: {}", file_to_delete);
            }

            let mut folder = PathBuf::new();
            folder.push(&session.host.folder);
            folder.push(file_split[0]);

            session.remove_folder(&folder)?;
            Ok(())
        };

        if self.indices.len() == 0 && self.files.len() == 0
        {
            for idx in 0..num_files
            {
                remove_idx(idx as usize)?;
            }
        }

        for idx in self.indices.iter() {
            let idx = if *idx < 0 { num_files + *idx } else { *idx } as usize;
            remove_idx(idx)?;
        }

        for file in &self.files {
            let mut folder = PathBuf::new();
            folder.push(&session.host.folder);

            let hash = get_hash(Path::new(file))?;
            folder.push(&hash);
            session.remove_folder(&folder)?;
        }

        Ok(())
    }
}
