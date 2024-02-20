use anyhow::{bail, Context, Result};
use clap::Parser;
use log::debug;
use std::io::IsTerminal;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::string::String;

use crate::at::At;
use crate::cfg::Config;
use crate::cli::color;
use crate::cli::WaitingSpinner;
use crate::cmd::Command;
use crate::ssh::SshSession;
use crate::util::get_hash;

/// Upload new files.
#[derive(Parser, Debug)]
pub struct Push {
    /// Alias/file name on the remote site.
    ///
    /// If you specify multiple files to upload you can either specify no aliases or as many
    /// aliases as there are files to upload.
    #[clap(short, long)]
    alias: Vec<String>,

    /// Expire the uploaded file after the given amount of time via `at`-scheduled remote job.
    ///
    /// Select files newer than the given duration. Durations can be: seconds (sec, s), minutes
    /// (min, m), days (d), weeks (w), months (M) or years (y).
    ///
    /// Mininum time till expiration is a minute.
    ///
    /// Any setting specified via command line overwrites settings from config files.
    ///
    /// A globally set expiration setting can overwritten by specifying "none".
    #[clap(short, long)]
    expire: Option<String>,

    /// File(s) to upload.
    #[clap()]
    files: Vec<PathBuf>,

    /// Limit upload speed (in Mbit/s). Please note that the upload speed will be shown in
    /// {M,K}Bytes/s, but most internet providers specify upload speeds in Mbits/s. This option
    /// makes it easier to specify what portion of your available upload speed to use.
    /// See also: --limit-kbytes
    #[clap(
        short = 'l',
        long,
        conflicts_with = "limit-kbytes",
        value_name = "Mbit/s"
    )]
    limit_mbits: Option<f64>,

    /// Limit upload speed (in kByte/s).
    #[clap(
        short = 'L',
        long,
        conflicts_with = "limit-mbits",
        value_name = "kByte/s"
    )]
    limit_kbytes: Option<f64>,

    /// Upload all files with the given prefix prepended.
    /// This is especially useful to give a bunch of files with generic names (e.g., plots) more
    /// context.
    ///
    /// Example: `--prefix foo_` causes `bar.png` to be uploaded as `foo_bar.png`.
    #[clap(short, long, conflicts_with = "alias")]
    prefix: Option<String>,

    /// Upload all files with the given suffix appended while not altering the file extension.
    /// This is especially useful to give a bunch of files with generic names (e.g., plots) more
    /// context.
    ///
    /// NOTE: Only the last extension is honored.
    ///
    /// Example: `--suffix _bar` causes `foo.png` to be uploaded as `foo_bar.png`.
    #[clap(short, long, conflicts_with = "alias")]
    suffix: Option<String>,
}

impl Push {
    fn upload(
        &self,
        session: &SshSession,
        config: &Config,
        to_upload: &Path,
        target_name: &str,
    ) -> Result<()> {
        let mut target = PathBuf::new();
        let prefix_length = session.host.prefix_length;
        let hash = get_hash(to_upload, prefix_length)
            .with_context(|| format!("Could not read {} to compute hash.", to_upload.display()))?;

        let expirer = if let Some(delay) = self
            .expire
            .as_ref()
            .or_else(|| session.host.expire.as_ref())
        {
            // Allow for explicit disabling term that overwrites a possibly set default
            if ["no", "none", "disabled", "false"].contains(&delay.as_str()) {
                None
            } else {
                Some(At::new(session, &delay)?)
            }
        } else {
            None
        };

        target.push(&hash);
        let folder = target.clone();
        session.make_folder(&folder)?;

        target.push(target_name);

        // TODO: Maybe check if file exists already.
        session.upload_file(
            &to_upload,
            &target,
            self.limit_mbits
                .map(|f| {
                    (f * 1024.0 /* mega */ * 1024.0/* kilo */ / 8.0/* bit -> bytes */) as usize
                })
                .or_else(|| {
                    self.limit_kbytes.map(|f| {
                        (f * 1024.0/* kilo */) as usize
                    })
                }),
        )?;

        if config.verify_via_hash {
            debug!("Verifying upload..");
            let spinner = WaitingSpinner::new("Verifying upload..".to_string());

            let remote_hash = session.get_remote_hash(&target, prefix_length)?;
            if hash != remote_hash {
                session.remove_folder(&folder)?;
                bail!(
                    "[{}] Hashes differ: local={} remote={}",
                    to_upload.display(),
                    hash,
                    remote_hash
                );
            }
            spinner.finish();
            debug!("Done");
        }

        if let Some(group) = &session.host.group {
            session.adjust_group(&folder, &group)?;
        };

        let expiration_date = if let Some(expirer) = expirer {
            Some(expirer.expire(&target)?)
        } else {
            None
        };
        io::stdout().flush().unwrap();
        // Only print expiration notification if asfa is used directly via terminal
        if let (true, Some(expiration_date)) = (std::io::stdout().is_terminal(), expiration_date) {
            eprint!(
                "{bl}expiring: {date}{br} ",
                bl = color::frame.apply_to("["),
                br = color::frame.apply_to("]"),
                date = color::expire.apply_to(expiration_date.to_rfc2822())
            );
        }
        println!(
            "{}",
            session
                .host
                .get_url(&format!("{}/{}", &hash, &target_name))?,
        );

        Ok(())
    }

    fn transform_filename(&self, file: &Path) -> Result<String> {
        let stem = file
            .file_stem()
            .with_context(|| format!("{} has no filename.", file.display()))?
            .to_str()
            .with_context(|| format!("Invalid filename: {}", file.display()))?;
        let extension = file
            .extension()
            .map(|ext| format!(".{}", ext.to_str().unwrap()))
            .unwrap_or_default();
        Ok(format!(
            "{prefix}{stem}{suffix}{ext}",
            prefix = self.prefix.as_ref().unwrap_or(&String::new()),
            stem = stem,
            suffix = self.suffix.as_ref().unwrap_or(&String::new()),
            ext = extension
        ))
    }
}

impl Command for Push {
    fn run(&self, session: &SshSession, config: &Config) -> Result<()> {
        let (files, aliases) = {
            let mut aliases: Vec<String> = vec![];
            let mut files: Vec<PathBuf> = vec![];

            if self.files.is_empty() && self.alias.is_empty() {
                bail!("No files to upload specified.");
            } else if self.files.is_empty() && !self.alias.is_empty() {
                if self.alias.len() == 2 {
                    // The other specified `asfa push --alias <alias> <file>`, clap is not able to
                    // parse this, so we fix it manually.
                    files.push(PathBuf::from(&self.alias[1]));
                    aliases.push(self.alias[0].clone());
                } else {
                    bail!(
                        "No files to upload specified. \
                        Did you forget to separate --alias option via double dashes from files to upload?"
                    );
                }
            } else if !self.alias.is_empty() && self.alias.len() != self.files.len() {
                bail!("You need to specify as many aliases as you specify files!");
            } else if self.alias.is_empty() {
                for file in self.files.iter() {
                    aliases.push(self.transform_filename(file)?);
                    files.push(file.clone());
                }
            } else {
                aliases = self.alias.clone();
                files = self.files.clone();
            }
            (files, aliases)
        };

        if let Some(limit) = self.limit_mbits {
            debug!("Limiting upload to {} Mbit/s", limit);
        }
        if let Some(limit) = self.limit_kbytes {
            debug!("Limiting upload to {} kByte/s", limit);
        }

        for (to_upload, alias) in files.iter().zip(aliases.iter()) {
            self.upload(session, config, to_upload, alias)?;
        }

        Ok(())
    }
}
