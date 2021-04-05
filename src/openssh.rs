use anyhow::Result;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Debug)]
enum OpenSshConfigEntry {
    /// Single value
    Single(String),

    /// Multiple values
    Multiple(Vec<String>),
}

#[derive(Debug)]
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
            let mut l = l.split(' ');
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
        log::trace!("openSSH config contents:\n{:#?}", raw);

        Ok(Self { raw })
    }

    fn check_prerequisites() -> std::io::Result<bool> {
        let status = Command::new("which")
            .arg("ssh")
            .stdout(Stdio::null())
            .status()?;
        Ok(status.success())
    }

    pub fn hostname(&self) -> Option<String> {
        let hostname = if let Some(OpenSshConfigEntry::Single(hostname)) = self.raw.get("hostname")
        {
            Some(hostname.to_string())
        } else {
            None
        };

        if let Some(port) = self.port() {
            hostname.map(|h| format!("{}:{}", h, port))
        } else {
            hostname
        }
    }

    pub fn port(&self) -> Option<String> {
        if let Some(OpenSshConfigEntry::Single(port)) = self.raw.get("port") {
            Some(port.to_string())
        } else {
            None
        }
    }

    pub fn private_key_files(&self) -> Vec<&str> {
        match self.raw.get("identityfile") {
            Some(entry) => entry.into(),
            None => Vec::new(),
        }
    }

    pub fn user(&self) -> Option<String> {
        if let Some(OpenSshConfigEntry::Single(user)) = self.raw.get("user") {
            Some(user.to_string())
        } else {
            None
        }
    }
}
