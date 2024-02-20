use anyhow::{Context, Result, bail};
use std::io::IsTerminal;
use clap::{AppSettings, Parser};
use console::Style;
use std::path::PathBuf;

use crate::cfg::Config;
use crate::cli::color;
use crate::cli::draw_boxed;
use crate::cmd::Command;
use crate::ssh::SshSession;

/// Rename an already uploaded file
#[derive(Parser, Debug)]
#[clap(global_setting=AppSettings::AllowNegativeNumbers)]
pub struct Rename {
    /// Show all details, can be set globally in config file.
    #[clap(long, short)]
    details: bool,

    /// Specify index of remote file or local file to compute hash from.
    #[clap()]
    input: String,

    /// New name to rename file
    #[clap()]
    filename: PathBuf,

    /// If `details` is set to true in config, --no-details can be specified to suppress output.
    #[clap(long, short = 'D')]
    no_details: bool,
}

enum IndexOrFile<'a> {
    Index(i64),
    Filename(&'a str),
}

impl Rename {
    fn parse_input(&self) -> IndexOrFile {
        use IndexOrFile::*;
        match self.input.parse::<i64>() {
            Ok(idx) => Index(idx),
            Err(_) => Filename(&self.input),
        }
    }
}

impl Command for Rename {
    fn run(&self, session: &SshSession, config: &Config) -> Result<()> {
        use IndexOrFile::*;
        let host = &session.host;

        let (input_indices, input_filenames) = {
            let mut indices = Vec::new();
            let mut filenames = Vec::new();

            match self.parse_input() {
                Index(idx) => {
                    indices.push(idx);
                }
                Filename(name) => {
                    filenames.push(name);
                }
            }

            (indices, filenames)
        };

        let remote_selected = session
            .list_files()?
            .by_hash(
                &input_filenames,
                session.host.prefix_length,
                /* bail_when_missing = */ false,
            )?
            .by_indices(&input_indices)?;

        if remote_selected.count() == 0 {
            match self.parse_input() {
                Index(idx) => {
                    bail!("Invalid remote index specified: {}", idx);
                }
                Filename(name) => {
                    bail!("File not uploaded to remote site: {}", name);
                }
            }
        } else if remote_selected.count() > 1 {
            bail!(
                "Found {} matching remote files, this should never happen.",
                remote_selected.count()
            );
        }

        let (_, old_path_relative, _) = remote_selected.iter().next().unwrap();

        let hash = old_path_relative
            .parent()
            .with_context(|| "Could not determine remote hash.")?;

        let path_old = {
            let mut path = host.folder.clone();
            path.push(old_path_relative);
            path
        };
        let path_new = {
            let mut path = host.folder.clone();
            path.push(&hash);
            path.push(&self.filename);
            path
        };

        session.exec_remote(&format!(
            "mv '{}' '{}'",
            path_old.display().to_string().replace("'", ""),
            path_new.display().to_string().replace("'", "")
        ))?;

        let url_new = host.get_url(&format!("{}/{}", hash.display(), &self.filename.display()))?;
        if !config.is_silent() {
            // Only print fancy boxes if we are attached to a TTY -> otherwise, just dump data in
            // parseable format
            if std::io::stdout().is_terminal() {
                let content = vec![format!(
                    " {old} → {new} ",
                    old = Style::new().red().bright().apply_to(
                        old_path_relative
                            .file_name()
                            .map(|s| s.to_string_lossy())
                            .with_context(|| "Invalid remote file name")?
                    ),
                    new = &url_new
                )];
                draw_boxed(
                    &Style::new()
                        .bold()
                        .green()
                        .bright()
                        .apply_to("Renaming:")
                        .to_string(),
                    content.iter().map(|s| s.as_ref()),
                    &color::frame,
                )?;
            } else {
                println!("{}", &url_new);
            }
        }
        Ok(())
    }
}
