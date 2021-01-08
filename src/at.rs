use crate::ssh::SshSession;

use anyhow::{bail, Context, Result};
use humantime::parse_duration;
use std::path::Path;
use std::time::Duration;

/// Wrapper to at-system on the remote side.
pub struct At<'a> {
    session: &'a SshSession<'a>,
    duration: Duration,
}

impl<'a> At<'a> {
    /// Check if `at` is available on the remote side and return wrapper.
    ///
    /// If it is, return Some(At), otherwise return None.
    pub fn new(session: &'a SshSession<'a>, human_duration: &str) -> Result<Self> {
        let duration = parse_duration(human_duration)
            .with_context(|| format!("Could not parse duration: {}", human_duration))?;

        if duration < Duration::from_secs(60) {
            bail!("Expiration delay needs to be at least one minute!");
        }

        let which = session.exec_remote("which at")?;

        match which.exit_status() {
            0 => Ok(Self { session, duration }),
            s => {
                log::debug!(
                    "Checking for `at` command returned {}. Stdout: {} Stderr: {}",
                    s,
                    which.stdout(),
                    which.stderr()
                );
                bail!("`at` command not available at remote site.");
            }
        }
    }

    /// Expire the given path relative to the remote base folder.
    ///
    /// First expires the file the parent folder.
    pub fn expire(&self, path: &Path) -> Result<()> {
        let stat = self
            .session
            .stat_single(path)
            .with_context(|| "File to expire missing.")?;

        if !stat.is_file() {
            bail!("Object to expire is no file: {}", path.display());
        }

        let tempfile = self.session.mktemp()?;

        let cmd_rm = format!(
            "#!/bin/bash\nrm '{}' && rmdir '{}'",
            self.session.prepend_base_folder(path).display(),
            self.session
                .prepend_base_folder(path.parent().with_context(|| format!(
                    "Could not determine parent folder of {}",
                    path.display()
                ))?)
                .display()
        );

        tempfile.write_str(&cmd_rm)?;

        let cmd_at = format!(
            "at -f '{}' now + {} minutes",
            tempfile.path().display(),
            self.num_mins()
        );

        self.session
            .exec_remote(&cmd_at)?
            .expect("Could not set remote expiration.")?;

        tempfile.remove()
    }

    fn num_mins(&self) -> u64 {
        self.duration.as_secs() / 60
    }
}