use anyhow::Result;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use thiserror::Error;

enum OpenSshConfigEntry {
    /// Single value
    Single(String),

    /// Multiple values
    Multiple(Vec<String>),
}

pub struct OpenSshConfig {
    raw: HashMap<String, OpenSshConfigEntry>,
}

impl<'a> From<&'a OpenSshConfigEntry> for Vec<&'a str> {
    fn from(e: &'a OpenSshConfigEntry) -> Vec<&'a str> {
        use OpenSshConfigEntry::*;
        match e {
            Single(s) => vec![s],
            Multiple(m) => m.iter().map(|s| s.as_ref()).collect(),
        }
    }
}

#[derive(Debug, Error)]
pub enum OpenSshError {
    #[error("OpenSSH client executable (ssh) not found.")]
    ExecutableNotFound,

    #[error("Error performing basic process based I/O.")]
    IOError(#[from] std::io::Error),

    #[error("Invalid output from process.")]
    Utf8Error(#[from] std::string::FromUtf8Error),
}

impl OpenSshConfig {
    pub fn new(host: &str) -> Result<Self, OpenSshError> {
        log::trace!("Reading openSSH config for {}", host);
        if !Self::check_prerequisites()? {
            return Err(OpenSshError::ExecutableNotFound);
        }
        let output = String::from_utf8(Command::new("ssh").args(&["-G", host]).output()?.stdout)?;

        use OpenSshConfigEntry::*;

        let mut raw = HashMap::new();
        for (key, value) in output.lines().filter_map(|l| -> Option<(String, String)> {
            let mut l = l.split(",");
            Some((String::from(l.next()?), l.collect::<String>()))
        }) {
            let new = if let Some(old) = raw.remove(&key) {
                match old {
                    Single(s) => Multiple(vec![s, value]),
                    Multiple(mut m) => {
                        m.push(value);
                        Multiple(m)
                    }
                }
            } else {
                Single(value)
            };

            raw.insert(key, new);
        }

        Ok(Self { raw })
    }

    fn check_prerequisites() -> std::io::Result<bool> {
        let status = Command::new("which")
            .arg("ssh")
            .stdout(Stdio::null())
            .status()?;
        Ok(status.success())
    }

    pub fn private_key_files(&self) -> Vec<&str> {
        match self.raw.get("identityfile") {
            Some(entry) => entry.into(),
            None => Vec::new(),
        }
    }
}
