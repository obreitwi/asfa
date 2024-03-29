use crate::cfg::{Auth, Host};
use crate::file_listing::FileListing;
use crate::openssh::OpenSshConfig;

use anyhow::{bail, Context, Result};
use expanduser::expanduser;
use indicatif::{ProgressBar, ProgressIterator};
use itertools::Itertools;
use log::{debug, error, info};
use rpassword::prompt_password;
use ssh2::Session as RawSession;
use ssh2::{FileStat, KeyboardInteractivePrompt, Prompt};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufReader, Error as IOError, ErrorKind};
use std::iter::{IntoIterator, Iterator};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use whoami::username;

fn ensure_port(hostname: &str) -> String {
    log::debug!("Raw hostname: {}", hostname);
    if hostname.contains(':') {
        hostname.to_string()
    } else {
        format!("{}:22", hostname)
    }
}

pub struct SshSession<'a> {
    raw: RawSession,
    pub host: &'a Host,
    cfg_openssh: Option<OpenSshConfig>,
}

impl<'a> SshSession<'a> {
    /// Adjust the group of the remote target recursively
    pub fn adjust_group(&self, file: &Path, group: &str) -> Result<()> {
        let cmd = format!("chown -R :{} \"{}\"", group, file.display());
        let mut channel = self.raw.channel_session()?;

        channel.exec(&cmd).context("Could not adjust group.")?;
        Ok(())
    }

    /// List all files present (relative to the current host's base-folder).
    pub fn all_files(&self) -> Result<Vec<PathBuf>> {
        let files = self
            .exec_remote(&format!(
                "find '{}' -mindepth 2 -maxdepth 2 -type f -print0 | xargs -0r ls -1rt",
                self.host.folder.display()
            ))?
            .expect("Could not list remote files.")?;

        log::trace!("{}", files.stdout());

        Ok(files
            .stdout()
            .lines()
            .map(|s| {
                Path::new(s)
                    .strip_prefix(&self.host.folder)
                    .unwrap()
                    .to_path_buf()
            })
            .collect())
    }

    /// Try all defined authentication methods in order
    fn auth(&self, auth: &Auth) -> Result<()> {
        log::trace!("Authenticating…");
        let mut methods = self.get_auth_methods()?;

        let supports_pubkey = methods.contains("publickey");
        if auth.use_agent && supports_pubkey {
            if let Err(e) = self.auth_agent() {
                log::debug!("Agent authentication failed: {}", e);
            }
        }

        // Check private key from user configuration
        if !self.raw.authenticated() && supports_pubkey {
            if let Some(private_key_file) = auth.private_key_file.as_deref() {
                if let Err(e) = self.auth_private_key(
                    private_key_file,
                    auth.private_key_file_password.as_deref(),
                    &self.get_username(),
                    auth.interactive,
                ) {
                    log::debug!(
                        "Private key authenication for '{}' (seemingly) failed: {}",
                        private_key_file,
                        e
                    );
                }
            } else if auth.from_openssh {
                self.auth_private_keys_openssh();
            }
        }

        if !self.raw.authenticated() && methods.contains("password") {
            if let Some(password) = self.host.password.as_ref() {
                debug!("Authenticating with plaintext password.");
                if let Err(e) = self.raw.userauth_password(&self.get_username(), &password) {
                    log::debug!("Password authenication failed: {}", e);
                }
            }
        }

        if !self.raw.authenticated() {
            // Update auth methods to discover keyboard-interactive as possible second
            // authentication step
            methods = self.get_auth_methods()?;
        }

        if !self.raw.authenticated() && auth.interactive && methods.contains("password") {
            if let Err(e) = self.auth_interactive() {
                log::debug!("Interactive password authenication failed: {}", e);
            }
        }

        if !self.raw.authenticated() && auth.interactive && methods.contains("keyboard-interactive")
        {
            if let Err(e) = self.auth_keyboard_interactive() {
                log::debug!("Interactive password authenication failed: {}", e);
            }
        }

        if self.raw.authenticated() {
            Ok(())
        } else {
            let msg = format!(
                "No authentication method successful for host {}. Run in loglevel debug for clues.",
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
        log::debug!("Trying to authenticate via agent..");
        let mut agent = self.raw.agent().unwrap();
        agent.connect().unwrap();
        agent.list_identities().unwrap();

        let username = &self.get_username();

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
        debug!(
            "Interactive authentication enabled for host {}",
            self.host.alias
        );
        let password = prompt_password(&format!(
            "Interactive authentication enabled. Enter password for {}:",
            self.host.alias
        ))?;
        self.raw
            .userauth_password(&self.get_username(), &password)?;
        Ok(())
    }

    fn auth_keyboard_interactive(&self) -> Result<()> {
        Ok(self.raw.userauth_keyboard_interactive(
            &self.get_username(),
            &mut InteractivePrompt::default(),
        )?)
    }

    fn auth_private_key(
        &self,
        private_key_file: &str,
        private_key_file_password: Option<&str>,
        username: &str,
        interactive: bool,
    ) -> Result<()> {
        log::debug!(
            "Trying to authenticate via private key file: {}",
            private_key_file
        );
        let password = match private_key_file_password {
            Some(pw) => Some(pw.to_owned()),
            None if interactive => Some(prompt_password(&format!(
                "Interactive authentication enabled. Enter password for {}:",
                private_key_file
            ))?),
            None => None,
        };

        let private_key_file = &expanduser(private_key_file)?;
        self.raw
            .userauth_pubkey_file(username, None, private_key_file, password.as_deref())?;

        Ok(())
    }

    /// Try to authenticate with private keys defined in openssh
    ///
    /// Assumes that auth() already checked for supported pubkey authentication!
    fn auth_private_keys_openssh(&self) {
        if self.cfg_openssh.is_none() {
            log::trace!("No openSSH config found, skipping private key authentication.");
            return;
        }

        // Because requesting auth methods too often might lead to connection abort, try
        // in order until first key matches.
        // -> Revisit if more sophisticated authenticatoin with several keys in order is actually
        // needed.
        for private_key in self.cfg_openssh.as_ref().unwrap().private_key_files() {
            if !Path::new(private_key).exists() {
                continue;
            }
            if let Err(e) = self.auth_private_key(private_key, None, &self.get_username(), false) {
                log::debug!(
                    "Private key authenication for '{}' (seemingly) failed: {}",
                    private_key,
                    e
                );
            }
            if self.raw.authenticated() {
                break;
            }
        }
    }

    /// Create a new SSH session by connecting to the given host configuration.
    ///
    /// First try authenticating with all agent identities then use an interactive password, if enabled.
    pub fn connect(host: &'a Host) -> Result<Self> {
        let auth: &Auth = &host.auth;

        let cfg_openssh = {
            match OpenSshConfig::new(
                host.hostname
                    .clone()
                    .map(|h| h.split(':').next().unwrap().to_string())
                    .unwrap_or_else(|| host.alias.clone())
                    .as_str(),
            ) {
                Ok(cfg) => Some(cfg),
                Err(e) => {
                    log::debug!("Could not load openSSH-config for host: {}", e);
                    None
                }
            }
        };

        let tcp = {
            let hostname = {
                // Priority:
                // 1. Fully specified hostname with port.
                // 2. Information from openSSH
                // 3. Hostname/alias
                if host.hostname.is_some() && host.hostname.clone().unwrap().contains(':') {
                    host.hostname.clone().unwrap()
                } else if let Some(hostname) = cfg_openssh.as_ref().and_then(|c| c.hostname()) {
                    hostname
                } else {
                    host.hostname.clone().unwrap_or_else(|| host.alias.clone())
                }
            };
            let full_hostname = ensure_port(&hostname);
            log::debug!("Connecting to: {}", full_hostname);
            TcpStream::connect(full_hostname)?
        };

        let mut sess = RawSession::new()?;
        sess.set_tcp_stream(tcp);
        sess.handshake()?;

        let ssh_session = SshSession {
            raw: sess,
            host,
            cfg_openssh,
        };

        ssh_session.auth(auth)?;

        if ssh_session.raw.authenticated() {
            log::trace!("Authenticated.");
            Ok(ssh_session)
        } else {
            error!("Could not authenticate ssh session, check your authentication settings!");
            Err(anyhow::Error::new(IOError::new(
                ErrorKind::PermissionDenied,
                "Authentication failed.",
            )))
        }
    }

    pub fn exec_remote(&self, cmd: &str) -> Result<ExecutedRemoteCommand> {
        ExecutedRemoteCommand::new(self, cmd)
    }

    /// Get listing of files
    pub fn list_files(&self) -> Result<FileListing> {
        FileListing::new(&self)
    }

    /// Make folder on the remote site if it does not exist (relative to the current host's
    /// base-folder).
    pub fn make_folder(&self, path: &Path) -> Result<()> {
        let path = self.prepend_base_folder(path);
        let path_str = path.display();
        let folder_exists = {
            let cmd = self.exec_remote(&format!("[ -d \"{}\" ]", path_str))?;

            cmd.exit_status() == 0
        };

        if !folder_exists {
            let cmd = self.exec_remote(&format!("mkdir \"{}\"", path_str))?;
            if cmd.exit_status() != 0 {
                bail!(
                    "Could not create remote folder: {} Error: {}",
                    path_str,
                    cmd.stderr()
                )
            }
        }
        Ok(())
    }

    /// Make remote file on remote side and return path to it.
    pub fn mktemp(&self) -> Result<Tempfile> {
        let tmp = self
            .exec_remote("mktemp")?
            .expect("Could not create temporary remote file.")?;
        Ok(Tempfile::new(self, PathBuf::from(tmp.stdout().trim_end())))
    }

    /// Get all available authentication methods
    pub fn get_auth_methods(&self) -> Result<HashSet<String>> {
        log::trace!("Getting auth methods.");
        let methods = self
            .raw
            .auth_methods(&self.get_username())?
            .split(',')
            .map(String::from)
            .collect();

        log::trace!("Advertised auth methods: {:?}", methods);

        Ok(methods)
    }

    /// Get username either from host, openSSH config or host system.
    pub fn get_username(&self) -> String {
        self.host
            .user
            .clone()
            .or_else(|| self.cfg_openssh.as_ref().and_then(|c| c.user()))
            .unwrap_or_else(username)
    }

    /// Get hash of the remote file (relative to the current host's base-folder).
    pub fn get_remote_hash(&self, path: &Path, length: u8) -> Result<String> {
        let path = [path];
        self.get_remote_hashes(&path[..], length)
            .map(|v| v.into_iter().next().unwrap())
    }

    /// Get hash of the remote file (relative to the current host's base-folder).
    pub fn get_remote_hashes(&self, paths: &[&Path], length: u8) -> Result<Vec<String>> {
        let mut paths: Vec<String> = paths
            .iter()
            .map(|p| self.prepend_base_folder(p))
            .map(|p| format!("\"{}\"", p.display()))
            .collect();
        let num_paths = paths.len();
        let hasher = if length == 0 {
            bail!("Length cannot be zero!");
        } else if length <= 32 {
            "sha256sum"
        } else if length <= 64 {
            "sha512sum"
        } else {
            bail!("Length should be equal to or smaller than 64.");
        };
        paths.insert(0, hasher.to_string());

        let cmd = paths.join(" ");
        let cmd_remote_hashes = self.exec_remote(&cmd)?.expect_with(|rc| match rc {
            127 => format!("{} not found on remote site.", hasher),
            _ => String::from("Unexpected remote error."),
        })?;
        let hashes: Vec<_> = cmd_remote_hashes
            .stdout
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .filter_map(|h| hex::decode(h).ok())
            .map(|h| base64::encode_config(h, base64::URL_SAFE)[..length as usize].to_string())
            .collect();

        if hashes.len() != num_paths {
            bail!("Computed {} hashes for {} paths.", hashes.len(), num_paths);
        }

        Ok(hashes)
    }

    pub fn prepend_base_folder(&self, path: &Path) -> PathBuf {
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

    /// Get stats about a remote files (relative to the current host's base-folder)
    pub fn stat<'b, I: IntoIterator<Item = &'b Path> + Clone>(
        &self,
        paths: I,
    ) -> Result<Vec<FileStat>> {
        if self.stat_bulk_available()? {
            self.stat_bulk(paths)
        } else {
            self.stat_fallback(paths)
        }
    }

    /// Get stats about a remote files (relative to the current host's base-folder)
    ///
    /// Faster version getting relevant information en bulk via find and xargs.
    pub fn stat_bulk<'b, I: IntoIterator<Item = &'b Path> + Clone>(
        &self,
        paths: I,
    ) -> Result<Vec<FileStat>> {
        // It is easier to simply check all files and then filter later..
        let mut channel = self.raw.channel_session()?;
        let cmd = format!(
            "find '{}' -mindepth 2 -maxdepth 2 -type f -print0 | xargs -0 stat -c '%Y %s %n'",
            &self.host.folder.display()
        );
        channel.exec(&cmd)?;
        let mut raw = String::new();
        channel.read_to_string(&mut raw)?;

        // Generate stats for all retrieved files
        let stats_map: HashMap<_, _> = raw
            .lines()
            .map(|l| {
                let mut parts = l.split(' ');
                let mtime: Option<u64> = parts.next().and_then(|s| s.parse().ok());
                let size: Option<u64> = parts.next().and_then(|s| s.parse().ok());
                let name: String = parts.join(" ");
                let path = PathBuf::from(&name);
                (
                    path,
                    FileStat {
                        size,
                        uid: None,
                        gid: None,
                        perm: None,
                        atime: None,
                        mtime,
                    },
                )
            })
            .collect();

        let num_paths = paths.clone().into_iter().count();

        // Re-order results to match requested files
        let stats: Vec<_> = paths
            .into_iter()
            .filter_map(|p| stats_map.get(&self.prepend_base_folder(p)).cloned())
            .collect();

        if stats.len() != num_paths {
            bail!("Expected {} stats, only got {}.", num_paths, stats.len());
        }
        Ok(stats)
    }

    /// Get stats about a remote files (relative to the current host's base-folder)
    ///
    /// Slower fallback that only relies on sftp functionality.
    pub fn stat_fallback<'b, I: IntoIterator<Item = &'b Path>>(
        &self,
        paths: I,
    ) -> Result<Vec<FileStat>> {
        debug!("Getting remote stats (fallback)…");
        let paths: Vec<_> = paths.into_iter().collect();

        let bar = ProgressBar::new(paths.len() as u64);
        bar.set_style(
            crate::cli::style_progress_bar_count().expect("couldn't create progress bar"),
        );
        bar.set_message("Getting file stats (fallback): ");

        let sftp = self.raw.sftp()?;
        let mut filestats = Vec::with_capacity(paths.len());
        for elem in paths.iter().progress_with(bar) {
            filestats.push(sftp.stat(&self.prepend_base_folder(elem))?);
        }
        debug!("Getting remote stats (fallback)… done");
        Ok(filestats)
    }

    /// Get stat for a single remote file (relative to base folder).
    pub fn stat_single(&self, path: &Path) -> Result<FileStat> {
        let path = self.prepend_base_folder(path);
        let sftp = self.raw.sftp()?;
        Ok(sftp.stat(&path)?)
    }

    /// Upload the given local path to the given remote path (relative to the current host's
    /// base-folder)
    pub fn upload_file(
        &self,
        path_local: &Path,
        path_remote: &Path,
        limit_speed_bytes_per_second: Option<usize>,
    ) -> Result<()> {
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

        let mut remote_file = {
            match self.raw.scp_send(&path_remote, 0o644, size, None) {
                Ok(file) => file,
                Err(error) => bail!(format!(
                    "Could not create remote file: {} Error: {}",
                    path_remote.display(),
                    error
                )),
            }
        };

        let bar = ProgressBar::new(local_file.metadata()?.len());
        bar.set_style(
            crate::cli::style_progress_bar_transfer().expect("couldn't create progress bar"),
        );
        let mut reader = BufReader::new(local_file);

        let start = Instant::now();
        let mut written_total = 0;
        let mut ui_update_last = Instant::now();
        let mut ui_update_written: u128 = 0;
        let ui_update_every = Duration::from_millis(250);

        let timestep = Duration::from_millis(50);

        loop {
            let now = Instant::now();
            let buf = &reader.fill_buf()?;
            let to_write = match limit_speed_bytes_per_second {
                None => buf.len(),
                Some(limit_bytes_per_sec) => {
                    let total_duration = now.duration_since(start);
                    if total_duration.as_micros() == 0 {
                        buf.len()
                    } else {
                        let current_avg_bytes_per_sec =
                            written_total * 1_000_000 / total_duration.as_micros();

                        if current_avg_bytes_per_sec > limit_bytes_per_sec as u128 {
                            // crude limit -> if we exceed speed limit just sleep
                            std::thread::sleep(timestep);
                            continue;
                        }
                        std::cmp::min(
                            buf.len(),
                            timestep.as_millis() as usize * limit_bytes_per_sec / 1_000,
                        )
                    }
                }
            };
            log::trace!("Writing {} bytes", to_write);
            if to_write > 0 {
                let written = remote_file
                    .write(&buf[..to_write])
                    .context("Failed to write chunk to remote file.")?;

                if limit_speed_bytes_per_second.is_some() {
                    remote_file.flush()?;
                }
                reader.consume(written);
                log::trace!("Wrote {} bytes", written);
                written_total += written as u128;

                if now.duration_since(ui_update_last) > ui_update_every {
                    bar.inc((written_total - ui_update_written) as u64);
                    ui_update_written = written_total;
                    ui_update_last = Instant::now();
                }
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Check if necessary utilities for fast stat generation are available.
    fn stat_bulk_available(&self) -> Result<bool> {
        let mut channel = self.raw.channel_session()?;
        let check = "which find && which xargs && which stat";
        channel.exec(check)?;
        Ok(channel.exit_status()? == 0)
    }
}

/// Wrapper for executed remote commands
#[derive(Debug)]
pub struct ExecutedRemoteCommand {
    cmd: String,
    exit_status: i32,
    stdout: String,
    stderr: String,
}

impl ExecutedRemoteCommand {
    fn new(ssh: &SshSession, cmd: &str) -> Result<Self> {
        let mut channel = ssh.raw.channel_session()?;
        log::trace!("Executing remotely: {}", cmd);
        channel
            .exec(cmd)
            .with_context(|| format!("Could not execute: {}", cmd))?;
        let mut stdout = String::new();
        let mut stderr = String::new();
        channel.read_to_string(&mut stdout)?;
        channel.stderr().read_to_string(&mut stderr)?;
        channel.wait_close()?;

        let exit_status = channel.exit_status()?;

        let cmd = Self {
            cmd: cmd.to_string(),
            stdout,
            stderr,
            exit_status,
        };
        log::trace!("{:#?}", cmd);
        Ok(cmd)
    }

    pub fn exit_status(&self) -> i32 {
        self.exit_status
    }

    pub fn expect(self, msg: &str) -> Result<Self> {
        if self.exit_status != 0 {
            self.fail(msg)
        } else {
            Ok(self)
        }
    }

    pub fn expect_with<F>(self, fn_exit_code_to_msg: F) -> Result<Self>
    where
        F: Fn(i32) -> String,
    {
        if self.exit_status != 0 {
            let msg = fn_exit_code_to_msg(self.exit_status);
            self.fail(&msg)
        } else {
            Ok(self)
        }
    }

    fn fail(self, msg: &str) -> Result<Self> {
        log::debug!(
            "While executing '{}' returned {}. Stdout: {} Stderr: {}",
            self.cmd,
            self.exit_status,
            self.stdout,
            self.stderr
        );
        bail!("{}", msg);
    }

    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    pub fn stderr(&self) -> &str {
        &self.stderr
    }
}

/// Remote tempfile
pub struct Tempfile<'a> {
    path: PathBuf,
    session: &'a SshSession<'a>,
}

impl<'a> Tempfile<'a> {
    fn new(session: &'a SshSession, path: PathBuf) -> Self {
        Self { path, session }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn remove(&self) -> Result<()> {
        self.session
            .exec_remote(&format!(
                "[ -f '{}' ] && rm '{}'",
                self.path.display(),
                self.path.display()
            ))?
            .expect("Could not remove temporary file.")?;
        Ok(())
    }

    /// Write string slice directly into temporary file, replacing its contents.
    pub fn write_str(&self, content: &str) -> Result<()> {
        let size = content.len() as u64;
        let mut remote_file = self
            .session
            .raw
            .scp_send(&self.path, 0o755, size, None)
            .with_context(|| format!("Could not create remote file: {}", self.path.display()))?;

        remote_file.write_all(content.as_bytes())?;
        Ok(())
    }
}

struct InteractivePrompt {}

impl Default for InteractivePrompt {
    fn default() -> Self {
        Self {}
    }
}

impl KeyboardInteractivePrompt for InteractivePrompt {
    fn prompt<'a>(
        &mut self,
        username: &str,
        instructions: &str,
        prompts: &[Prompt<'a>],
    ) -> Vec<String> {
        debug!(
            "Performing keyboard-interactive auth{}.",
            if !username.is_empty() {
                format!(" for {}", username)
            } else {
                "".to_string()
            }
        );
        if !instructions.is_empty() {
            debug!("{}", instructions);
        }
        prompts
            .iter()
            .map(|p| {
                if p.echo {
                    dialoguer::Input::new()
                        .with_prompt(&p.text.to_string())
                        .allow_empty(true)
                        .interact_text()
                        .unwrap_or_default()
                } else {
                    prompt_password(&p.text).unwrap_or_default()
                }
            })
            .collect()
    }
}
