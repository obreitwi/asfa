use anyhow::{bail, Context, Result};
use clap::Clap;
use dialoguer::Confirm;
use log::debug;
use std::path::Path;

use crate::cfg::Config;
use crate::cli::color;
use crate::cmd::Command;
use crate::ssh::SshSession;

/// Clear already uploaded files.
#[derive(Clap, Debug)]
pub struct Clean {
    /// Clean all remote files (dangerous!)
    #[clap(long)]
    all: bool,

    /// Disable confirming deletions
    #[clap(long = "no-confirm")]
    no_confirm: bool,

    /// Explicit file to delete
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Indices of files to delete as returned by `list` command.
    #[clap()]
    indices: Vec<i64>,
}

impl Command for Clean {
    fn run(&self, session: &SshSession, _config: &Config) -> Result<()> {
        debug!("Cleaning remote files..");

        let files_to_delete = session.get_files_by(
            &self.indices,
            &self.files.iter().map(|s| s.as_str()).collect::<Vec<&str>>(),
            session.host.prefix_length,
        )?;

        let do_delete = self.no_confirm || {
            let dot = color::dot.apply_to("*");
            let formatted_files: Vec<String> = files_to_delete
                .files
                .iter()
                .map(|(_, f)| {
                    format!(
                        " {dot} {file} ",
                        dot = dot,
                        file = color::entry.apply_to(f.display())
                    )
                })
                .collect();

            crate::cli::draw_boxed(
                &format!(
                    "Will {delete} the following files:",
                    delete = console::Style::new()
                        .bold()
                        .red()
                        .bright()
                        .apply_to("delete")
                )
                .as_str(),
                formatted_files.iter().map(|s| s.as_str()),
                &color::frame,
            )?;
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
            for (_, file) in files_to_delete.files {
                remove_file(&file)?
            }
        }

        Ok(())
    }
}
