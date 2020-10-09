use anyhow::{bail, Context, Result};
use clap::Clap;
// use log::info;
use log::debug;
use std::path::{Path, PathBuf};
use std::string::String;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::cfg::Config;
use crate::cmd::Command;
use crate::ssh::SshSession;
use crate::util::get_hash;

/// Upload new files.
#[derive(Clap, Debug)]
pub struct Push {
    /// Alias/file name on the remote site.
    ///
    /// If you specify multiple files to upload you can either specify no aliases or as many
    /// aliases as there are files to upload.
    #[clap(short, long)]
    alias: Vec<String>,

    /// File(s) to upload.
    #[clap()]
    files: Vec<PathBuf>,
}

fn upload(
    session: &SshSession,
    config: &Config,
    to_upload: &Path,
    target_name: &str,
) -> Result<()> {
    let mut target = PathBuf::new();
    let prefix_length = session.host.prefix_length;
    let hash = get_hash(to_upload, prefix_length)
        .with_context(|| format!("Could not read {} to compute hash.", to_upload.display()))?;

    target.push(&hash);
    let folder = target.clone();
    session.make_folder(&folder)?;

    target.push(target_name);

    // TODO: Maybe check if file exists already.
    session.upload_file(&to_upload, &target)?;

    if config.verify_via_hash {
        debug!("Verifying upload..");
        let stop_token = Arc::new(Mutex::new(false));
        let stop_token_pbar = Arc::clone(&stop_token);
        let spinner = thread::spawn(move || {
            let spinner = crate::cli::spinner();
            spinner.set_message("Verifying upload..");

            while !*stop_token_pbar.lock().unwrap() {
                spinner.inc(1);
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            spinner.set_message("Verifying upload.. done");
            spinner.inc(1);
            spinner.finish_and_clear();
        });

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
        *stop_token.lock().unwrap() = true;
        spinner.join().unwrap();
        debug!("Done");
    }

    if let Some(group) = &session.host.group {
        session.adjust_group(&folder, &group)?;
    };

    println!(
        "{}",
        session
            .host
            .get_url(&format!("{}/{}", &hash, &target_name))?
    );
    Ok(())
}

impl Command for Push {
    fn run(&self, session: &SshSession, config: &Config) -> Result<()> {
        let mut aliases: Vec<String> = vec![];

        if self.files.len() == 0 && self.alias.len() == 0 {
            bail!("No files to upload specified.");
        } else if self.files.len() == 0 && self.alias.len() > 0 {
            bail!(
                "No files to upload specified. \
                  Did you forget to seperate --alias option via double dashes from files to upload?"
            );
        } else if self.alias.len() > 0 && self.alias.len() != self.files.len() {
            bail!("You need to specify as many aliases as you specify files!");
        } else if self.alias.len() == 0 {
            for file in self.files.iter() {
                aliases.push(
                    file.file_name()
                        .with_context(|| format!("{} has no filename.", file.display()))?
                        .to_str()
                        .with_context(|| format!("{} has invalid filename", file.display()))?
                        .to_string(),
                );
            }
        } else {
            aliases = self.alias.clone();
        }

        for (to_upload, alias) in self.files.iter().zip(aliases.iter()) {
            upload(session, config, to_upload, alias)?;
        }

        Ok(())
    }
}
