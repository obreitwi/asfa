use anyhow::{bail, Context, Result};
use clap::Clap;
use dialoguer::Confirm;
use log::debug;
use std::path::Path;

use crate::cfg::Config;
use crate::cli::color;
use crate::cmd::Command;
use crate::ssh::SshSession;
use crate::util::get_hash;

/// Clear already uploaded files.
#[derive(Clap, Debug)]
pub struct Clean {
    /// Clean all remote files (dangerous!)
    #[clap(long)]
    all: bool,

    /// Disable confirming deletions
    #[clap(long = "no-confirm")]
    no_confirm: bool,

    /// Excplicit file to delete
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Indices of files to delete as returned by `list` command.
    #[clap()]
    indices: Vec<i64>,
}

impl Command for Clean {
    fn run(&self, session: &SshSession, config: &Config) -> Result<()> {
        debug!("Cleaning remote files..");
        let remote_files = session.list_files()?;
        let num_files = remote_files.len() as i64;

        let mut files_to_delete: Vec<&Path> = Vec::new();

        if self.indices.len() == 0 && self.files.len() == 0 {
            for idx in 0..num_files {
                files_to_delete.push(&remote_files[idx as usize]);
            }
        }

        for idx in self.indices.iter() {
            let idx = if *idx < 0 { num_files + *idx } else { *idx } as usize;
            files_to_delete.push(&remote_files[idx as usize]);
        }

        for file in &self.files {
            let hash = get_hash(
                Path::new(file),
                session.host.prefix_length.unwrap_or(config.prefix_length),
            )?;
            for file in remote_files.iter() {
                if file.starts_with(&hash) {
                    files_to_delete.push(&file);
                    continue;
                }
            }
            bail!("No file with same hash found on server: {}", file);
        }

        let do_delete = self.no_confirm || {
            crate::cli::draw_boxed(
                "Will delete the following files:",
                &color::heading,
                &color::frame,
            )?;

            for file in files_to_delete.iter() {
                println!(
                    " {dot} {file}",
                    dot = color::dot.apply_to("*"),
                    file = color::entry.apply_to(file.display())
                )
            }
            println!("");
            Confirm::new()
                .with_prompt("Delete files?")
                .default(false)
                .interact()?
        };

        let remove_file =
            |file_to_delete: &Path| -> Result<()> {
                if file_to_delete.components().count() != 2 {
                    bail!("Invalid filename: {}", file_to_delete.display());
                }

                session.remove_folder(file_to_delete.parent().with_context(|| {
                    format!("File had not parent: {}", file_to_delete.display())
                })?)?;
                Ok(())
            };

        if do_delete {
            for file in files_to_delete {
                remove_file(file)?
            }
        }

        Ok(())
    }
}
