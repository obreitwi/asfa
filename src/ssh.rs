use crate::cfg::{Auth, Host};

use anyhow::{bail, Context, Result};
use indicatif::ProgressBar;
use log::{debug, error, info};
use rpassword::prompt_password_stderr;
use ssh2::Session as RawSession;
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufReader, Error as IOError, ErrorKind};
use std::net::TcpStream;
use std::path::Path;

fn ensure_port(hostname: &str) -> String {
    if hostname.contains(":") {
        hostname.to_string()
    } else {
        format!("{}:22", hostname)
    }
}

pub struct SshSession<'a> {
    raw: RawSession,
    pub host: &'a Host,
}

impl<'a> SshSession<'a> {
    fn auth(&self, auth: &Auth) -> Result<()> {
        if let Some(password) = self.host.password.as_ref() {
            debug!("Authenticating with plaintext password.");
            self.raw
                .userauth_password(&self.host.get_username(), &password)?;
            Ok(())
        } else if auth.use_agent {
            self.auth_agent()?;
            Ok(())
        } else if auth.interactive {
            self.auth_interactive()?;
            Ok(())
        } else {
            let msg = format!(
                "No authentication method defined for host {}",
                self.host.alias
            );
            error!("{}", &msg);
            Err(anyhow::Error::new(IOError::new(
                ErrorKind::InvalidInput,
                msg,
            )))
        }
    }

    fn auth_agent(&self) -> Result<()> {
        let mut agent = self.raw.agent().unwrap();
        agent.connect().unwrap();
        agent.list_identities().unwrap();

        let username = &self.host.get_username();

        for identity in agent.identities().unwrap() {
            debug!("Trying {}", identity.comment());
            if let Ok(()) = agent.userauth(username, &identity) {
                return Ok(());
            };
        }
        Err(anyhow::Error::new(IOError::new(
            ErrorKind::PermissionDenied,
            "All pubkeys failed to authenticate.",
        )))
    }

    fn auth_interactive(&self) -> Result<()> {
        info!(
            "Interactive authentication enabled for host {}",
            self.host.alias
        );
        let password = prompt_password_stderr(&format!(
            "Interactive authentication enabled. Enter password for {}:",
            self.host.alias
        ))?;
        self.raw
            .userauth_password(&self.host.get_username(), &password)?;
        Ok(())
    }

    /// Adjust the group of the remote target recursively
    pub fn adjust_group(&self, file: &Path, group: &str) -> Result<()> {
        let cmd = format!("chown -R :{} \"{}\"", group, file.display());
        let mut channel = self.raw.channel_session()?;

        channel.exec(&cmd).context("Could not adjust group.")?;
        Ok(())
    }

    /// Create a new SSH session from the given host configuration.
    ///
    /// First try authenticating with all agent identities then use an interactive password, if enabled.
    pub fn create(host: &'a Host, global_auth_cfg: &Auth) -> Result<Self> {
        let auth: &Auth = host.auth.as_ref().unwrap_or(global_auth_cfg);

        let tcp = TcpStream::connect(ensure_port(host.get_hostname()))?;

        let mut sess = RawSession::new()?;
        sess.set_tcp_stream(tcp);
        sess.handshake()?;

        let ssh_session = SshSession { raw: sess, host };

        ssh_session.auth(auth)?;

        if ssh_session.raw.authenticated() {
            Ok(ssh_session)
        } else {
            error!("Could not authenticate ssh session, check your authentication settings!");
            Err(anyhow::Error::new(IOError::new(
                ErrorKind::PermissionDenied,
                "Authentication failed.",
            )))
        }
    }

    /// List all files present
    pub fn list_files(&self, folder_remote: &Path) -> Result<Vec<String>> {
        let cmd = format!("cd {} && ls -1rt */*", folder_remote.display());
        let mut channel = self.raw.channel_session()?;

        channel
            .exec(&cmd)
            .context("Could not execute listing command")?;
        let mut files = String::new();
        channel.read_to_string(&mut files)?;
        Ok(files.lines().map(|s| s.to_string()).collect())
    }

    /// Make folder on the remote site if it does not exist.
    pub fn make_folder(&self, path: &Path) -> Result<()> {
        let path_str = path.display();
        let cmd = format!("[ ! -d \"{}\" ] && mkdir \"{}\"", path_str, path_str);
        let mut channel = self.raw.channel_session()?;
        channel
            .exec(&cmd)
            .with_context(|| format!("Could not create remote folder: {}", path_str))
    }

    /// Get hash of the remote file
    pub fn get_remote_hash(&self, path: &Path, length: u8) -> Result<String> {
        let hasher = if length == 0 {
            bail!("Length cannot be zero!");
        } else if length <= 32 {
            "sha256sum"
        } else if length <= 64 {
            "sha512sum"
        } else {
            bail!("Length should be smaller than 64.");
        };

        let mut channel = self.raw.channel_session()?;
        let cmd = format!("{} \"{}\"", hasher, path.display());
        channel.exec(&cmd)?;
        let mut stdout = String::new();
        let mut stderr = String::new();
        channel.read_to_string(&mut stdout)?;
        channel.stderr().read_to_string(&mut stderr)?;
        channel.wait_close()?;
        match channel.exit_status()? {
            0 => { /* just continue */ }
            127 => bail!("{} not found on remote site.", hasher),
            s => bail!(
                "Computing remote hash exited with {}. Stdout: {} Stderr: {}",
                s,
                stdout,
                stderr
            ),
        }
        let full_hash = stdout
            .split_whitespace()
            .next()
            .context("No hash found in output.")?;
        Ok(full_hash[..length as usize].to_string())
    }

    /// Remove the given folder and its contents
    pub fn remove_folder(&self, path: &Path) -> Result<()> {
        let path_str = path.display();
        debug!("Removing: {}", path_str);
        let cmd = format!("[ -d \"{}\" ] && rm -rvf \"{}\"", path_str, path_str);
        let mut channel = self.raw.channel_session()?;
        channel
            .exec(&cmd)
            .with_context(|| format!("Could not remove remote folder: {}", path_str))?;
        let mut s = String::new();
        channel.read_to_string(&mut s)?;
        for l in s.lines() {
            info!("{}", l);
        }
        Ok(())
    }

    /// Upload the given local path to the given remote path
    pub fn upload_file(&self, path_local: &Path, path_remote: &Path) -> Result<()> {
        debug!(
            "Uploading: {} -> {}",
            path_local.display(),
            path_remote.display()
        );
        let local_file = File::open(path_local).context("Could not open local file.")?;
        let size = local_file
            .metadata()
            .context("Could not get metadata of local file.")?
            .len();

        let mut remote_file = self
            .raw
            .scp_send(path_remote, 0o644, size, None)
            .with_context(|| format!("Could not create remote file: {}", path_remote.display()))?;

        let bar = ProgressBar::new(local_file.metadata()?.len());
        let mut reader = BufReader::new(local_file);

        loop {
            let buf = &reader.fill_buf()?;
            let to_write = buf.len();
            if to_write > 0 {
                remote_file
                    .write(buf)
                    .context("Failed to write chunk to remote file.")?;
                debug!("Wrote {} bytes..", to_write);
                &reader.consume(to_write);
                bar.inc(to_write as u64);
            } else {
                break;
            }
        }

        Ok(())
    }
}
