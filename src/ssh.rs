use crate::cfg::{Auth, Host};
use crate::util;

use anyhow::{bail, Context, Result};
use indicatif::ProgressBar;
use log::{debug, error, info};
use rpassword::prompt_password_stderr;
use ssh2::Session as RawSession;
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufReader, Error as IOError, ErrorKind};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

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
        if auth.use_agent {
            self.auth_agent()?;
            Ok(())
        } else if let Some(private_key_file) = auth.private_key_file.as_deref() {
            self.auth_private_key(
                &private_key_file,
                auth.private_key_file_password.as_deref(),
                &self.host.get_username(),
                auth.interactive,
            )?;
            Ok(())
        } else if auth.interactive {
            self.auth_interactive()?;
            Ok(())
        } else if let Some(password) = self.host.password.as_ref() {
            debug!("Authenticating with plaintext password.");
            self.raw
                .userauth_password(&self.host.get_username(), &password)?;
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

    fn auth_private_key(
        &self,
        private_key_file: &str,
        private_key_file_password: Option<&str>,
        username: &str,
        interactive: bool,
    ) -> Result<()> {
        let password = match private_key_file_password {
            Some(pw) => Some(pw.to_owned()),
            None if interactive => Some(prompt_password_stderr(&format!(
                "Interactive authentication enabled. Enter password for {}:",
                private_key_file
            ))?),
            None => None,
        };

        self.raw.userauth_pubkey_file(
            username,
            None,
            Path::new(private_key_file),
            password.as_deref(),
        )?;

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

    /// List all files present (relative to the current host's base-folder).
    pub fn list_files(&self) -> Result<Vec<PathBuf>> {
        let cmd = format!("cd {} && ls -1rt */*", self.host.folder.display());
        let mut channel = self.raw.channel_session()?;

        channel
            .exec(&cmd)
            .context("Could not execute listing command")?;
        let mut files = String::new();
        channel.read_to_string(&mut files)?;
        Ok(files.lines().map(|s| Path::new(s).to_path_buf()).collect())
    }

    pub fn get_files_by(
        &self,
        indices: &[i64],
        names: &[&str],
        prefix_length: u8,
    ) -> Result<Vec<PathBuf>> {
        let remote_files = self.list_files()?;
        let num_files = remote_files.len() as i64;

        let mut selected: Vec<&Path> = Vec::new();

        if indices.len() == 0 && names.len() == 0 {
            for idx in 0..num_files {
                selected.push(&remote_files[idx as usize]);
            }
        }

        for idx in indices.iter() {
            let idx = if *idx < 0 { num_files + *idx } else { *idx } as usize;
            selected.push(&remote_files[idx as usize]);
        }

        'outer: for file in names {
            let hash = util::get_hash(Path::new(file), prefix_length)?;
            for file in remote_files.iter() {
                if file.starts_with(&hash) {
                    selected.push(&file);
                    continue 'outer;
                }
            }
            bail!("No file with same hash found on server: {}", file);
        }

        Ok(selected.iter().map(|&f| f.to_owned()).collect())
    }

    /// Make folder on the remote site if it does not exist (relative to the current host's
    /// base-folder).
    pub fn make_folder(&self, path: &Path) -> Result<()> {
        let path = self.prepend_base_folder(path);
        let path_str = path.display();
        let cmd = format!("[ ! -d \"{}\" ] && mkdir \"{}\"", path_str, path_str);
        let mut channel = self.raw.channel_session()?;
        channel
            .exec(&cmd)
            .with_context(|| format!("Could not create remote folder: {}", path_str))
    }

    /// Get hash of the remote file (relative to the current host's base-folder).
    pub fn get_remote_hash(&self, path: &Path, length: u8) -> Result<String> {
        let path = self.prepend_base_folder(path);
        let hasher = if length == 0 {
            bail!("Length cannot be zero!");
        } else if length <= 64 {
            "sha256sum"
        } else if length <= 128 {
            "sha512sum"
        } else {
            bail!("Length should be smaller than 128.");
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

    fn prepend_base_folder(&self, path: &Path) -> PathBuf {
        let mut buf = PathBuf::new();
        buf.push(&self.host.folder);
        buf.push(path);
        buf
    }

    /// Remove the given folder and its contents (relative to the current host's base-folder)
    pub fn remove_folder(&self, path: &Path) -> Result<()> {
        let path = self.prepend_base_folder(path);
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

    /// Upload the given local path to the given remote path (relative to the current host's
    /// base-folder)
    pub fn upload_file(&self, path_local: &Path, path_remote: &Path) -> Result<()> {
        let path_remote = self.prepend_base_folder(path_remote);
        debug!(
            "Uploading: '{}' → '{}'",
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
            .scp_send(&path_remote, 0o644, size, None)
            .with_context(|| format!("Could not create remote file: {}", path_remote.display()))?;

        let bar = ProgressBar::new(local_file.metadata()?.len());
        bar.set_style(crate::cli::style_progress_bar());
        let mut reader = BufReader::new(local_file);

        loop {
            let buf = &reader.fill_buf()?;
            let to_write = buf.len();
            if to_write > 0 {
                remote_file
                    .write(buf)
                    .context("Failed to write chunk to remote file.")?;
                &reader.consume(to_write);
                bar.inc(to_write as u64);
            } else {
                break;
            }
        }

        Ok(())
    }
}
