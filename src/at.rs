use crate::ssh::SshSession;

use anyhow::{bail, Context, Result};
use chrono::prelude::*;
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
    ///
    /// Returns the expected expiration date.
    pub fn expire(&self, path: &Path) -> Result<DateTime<Local>> {
        let stat = self
            .session
            .stat_single(path)
            .with_context(|| "File to expire missing.")?;

        if !stat.is_file() {
            bail!("Object to expire is no file: {}", path.display());
        }
        let now = Local::now();

        let tempfile = self.session.mktemp()?;

        let cmd_rm = format!(
            "#!/usr/bin/env bash\nrm '{}' && rmdir '{}'",
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

        let submission = self
            .session
            .exec_remote(&cmd_at)?
            .expect("Could not set remote expiration.")?;

        tempfile.remove()?;

        let pattern = "No atd running?";
        if submission.stderr().contains(&pattern) {
            bail!(
                "There was a problem setting the remote file to expire: \
                atd does not appear to be running. Please investigate!\n\
                Remote returned: {}",
                submission
                    .stderr()
                    .lines()
                    .filter(|l| l.contains(&pattern))
                    .collect::<Vec<&str>>()
                    .join("\n")
            );
        }

        Ok(now + chrono::Duration::from_std(self.duration)?)
    }

    fn num_mins(&self) -> u64 {
        self.duration.as_secs() / 60
    }
}
