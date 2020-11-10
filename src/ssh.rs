use crate::cfg::{Auth, Host};
use crate::openssh::OpenSshConfig;
use crate::util;

use anyhow::{bail, Context, Result};
use expanduser::expanduser;
use indicatif::{ProgressBar, ProgressIterator};
use itertools::Itertools;
use log::{debug, error, info};
use regex::Regex;
use rpassword::prompt_password_stderr;
use ssh2::Session as RawSession;
use ssh2::{FileStat, KeyboardInteractivePrompt, Prompt};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufReader, Error as IOError, ErrorKind};
use std::iter::{IntoIterator, Iterator};
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
        let cmd = format!("cd {} && ls -1rt */*", self.host.folder.display());
        let mut channel = self.raw.channel_session()?;

        channel
            .exec(&cmd)
            .context("Could not execute listing command")?;
        let mut files = String::new();
        channel.read_to_string(&mut files)?;
        Ok(files.lines().map(|s| Path::new(s).to_path_buf()).collect())
    }

    /// Try all defined authentication methods in order
    fn auth(&self, auth: &Auth) -> Result<()> {
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
                    &self.host.get_username(),
                    auth.interactive,
                ) {
                    log::debug!(
                        "Private key authenication for '{}' (seemingly) failed: {}",
                        private_key_file,
                        e
                    );
                }
            } else if auth.from_openssh {
                self.auth_private_keys_openssh()?;
            }
        }

        if !self.raw.authenticated() && methods.contains("password") {
            if let Some(password) = self.host.password.as_ref() {
                debug!("Authenticating with plaintext password.");
                if let Err(e) = self
                    .raw
                    .userauth_password(&self.host.get_username(), &password)
                {
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

    fn auth_keyboard_interactive(&self) -> Result<()> {
        Ok(self.raw.userauth_keyboard_interactive(
            &self.host.get_username(),
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
            None if interactive => Some(prompt_password_stderr(&format!(
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
    fn auth_private_keys_openssh(&self) -> Result<()> {
        let should_perform = || -> Result<bool> {
            Ok(self.get_auth_methods()?.contains("publickey") && !self.raw.authenticated())
        };

        if self.cfg_openssh.is_none() {
            log::trace!("No openSSH config found, skipping private key authentication.");
            return Ok(());
        }

        for private_key in self.cfg_openssh.as_ref().unwrap().private_key_files() {
            if !should_perform()? {
                return Ok(());
            }
            if !Path::new(private_key).exists() {
                continue;
            }
            if let Err(e) =
                self.auth_private_key(private_key, None, &self.host.get_username(), false)
            {
                log::debug!(
                    "Private key authenication for '{}' (seemingly) failed: {}",
                    private_key,
                    e
                );
            }
        }
        Ok(())
    }

    /// Create a new SSH session from the given host configuration.
    ///
    /// First try authenticating with all agent identities then use an interactive password, if enabled.
    pub fn create(host: &'a Host) -> Result<Self> {
        let auth: &Auth = &host.auth;

        let cfg_openssh = {
            match OpenSshConfig::new(&host.alias) {
                Ok(cfg) => Some(cfg),
                Err(e) => {
                    log::debug!("Could not load openSSH-config for host: {}", e);
                    None
                }
            }
        };

        let tcp = TcpStream::connect(ensure_port(host.get_hostname()))?;

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
            Ok(ssh_session)
        } else {
            error!("Could not authenticate ssh session, check your authentication settings!");
            Err(anyhow::Error::new(IOError::new(
                ErrorKind::PermissionDenied,
                "Authentication failed.",
            )))
        }
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
        let cmd = format!("[ ! -d \"{}\" ] && mkdir \"{}\"", path_str, path_str);
        let mut channel = self.raw.channel_session()?;
        channel
            .exec(&cmd)
            .with_context(|| format!("Could not create remote folder: {}", path_str))
    }

    /// Get all available authentication methods
    pub fn get_auth_methods(&self) -> Result<HashSet<String>> {
        let methods = self
            .raw
            .auth_methods(&self.host.get_username())?
            .split(',')
            .map(String::from)
            .collect();

        log::trace!("Advertised auth methods: {:?}", methods);

        Ok(methods)
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

        let mut channel = self.raw.channel_session()?;
        let cmd = paths.join(" ");
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
        let hashes: Vec<_> = stdout
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

    /// Get stats about a remote files (relative to the current host's base-folder)
    pub fn stat<'b, I: IntoIterator<Item = &'b Path>>(&self, paths: I) -> Result<Vec<FileStat>> {
        if self.stat_bulk_available()? {
            self.stat_bulk(paths)
        } else {
            self.stat_fallback(paths)
        }
    }

    /// Get stats about a remote files (relative to the current host's base-folder)
    ///
    /// Faster version getting relevant information en bulk via find and xargs.
    pub fn stat_bulk<'b, I: IntoIterator<Item = &'b Path>>(
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
        let paths: HashSet<_> = paths
            .into_iter()
            .map(|p| {
                let path = self.prepend_base_folder(p);
                log::trace!("Requesting: {}", path.display());
                path
            })
            .collect();
        let stats: Vec<_> = raw
            .lines()
            .filter_map(|l| {
                let mut parts = l.split(" ");
                let mtime: Option<u64> = parts.next().and_then(|s| s.parse().ok());
                let size: Option<u64> = parts.next().and_then(|s| s.parse().ok());
                let name: String = parts.join(" ");
                if paths.contains(Path::new(&name)) {
                    Some(FileStat {
                        size,
                        uid: None,
                        gid: None,
                        perm: None,
                        atime: None,
                        mtime,
                    })
                } else {
                    log::trace!("Not requested: {}", name);
                    None
                }
            })
            .collect();

        if stats.len() != paths.len() {
            bail!("Expected {} stats, only got {}.", paths.len(), stats.len());
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
        bar.set_style(crate::cli::style_progress_bar_count());
        bar.set_message("Getting file stats (fallback): ");

        let sftp = self.raw.sftp()?;
        let mut filestats = Vec::with_capacity(paths.len());
        for elem in paths.iter().progress_with(bar) {
            filestats.push(sftp.stat(&self.prepend_base_folder(elem))?);
        }
        debug!("Getting remote stats (fallback)… done");
        Ok(filestats)
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
        bar.set_style(crate::cli::style_progress_bar_transfer());
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

    /// Check if necessary utilities for fast stat generation are available.
    fn stat_bulk_available(&self) -> Result<bool> {
        let mut channel = self.raw.channel_session()?;
        let check = "which find && which xargs && which stat";
        channel.exec(check)?;
        Ok(channel.exit_status()? == 0)
    }
}

/// Helper structure to avoid re-implementing file listing capabilities for all commands.
pub struct FileListing<'a> {
    pub num_files: usize,
    all_files: HashMap<usize, PathBuf>,
    pub indices: Vec<usize>,
    pub stats: Option<HashMap<usize, FileStat>>,
    ssh: &'a SshSession<'a>,
}

impl<'a> FileListing<'a> {
    fn new(ssh: &'a SshSession) -> Result<FileListing<'a>> {
        let all_files: HashMap<_, _> = ssh.all_files()?.into_iter().enumerate().collect();
        let num_files = all_files.len();

        Ok(Self {
            num_files,
            all_files,
            indices: Vec::new(),
            stats: None,
            ssh,
        })
    }

    /// Select all files which name matches regex
    pub fn by_filter(self, filter: Option<&str>) -> Result<Self> {
        match filter {
            None => Ok(self),
            Some(filter) => {
                let re = Regex::new(filter)?;
                let indices = {
                    let mut indices = self.indices;
                    let mut additions: Vec<_> = self
                        .all_files
                        .iter()
                        .map(|(idx, p)| (idx, p.as_path()))
                        .filter(|(_, path)| {
                            re.is_match(&path.file_name().unwrap().to_string_lossy().to_string())
                        })
                        .map(|(idx, _)| *idx)
                        .collect();

                    indices.append(&mut additions);
                    Self::make_unique(indices)
                };
                Ok(Self { indices, ..self })
            }
        }
    }

    /// Select all files with corresponding indices
    pub fn by_indices(self, indices: &[i64]) -> Result<Self> {
        if indices.len() > 0 {
            let num_files = self.num_files as i64;
            for idx in indices {
                if *idx < -num_files || *idx >= num_files {
                    bail!("Invalid index specified: {}", idx);
                }
            }

            let num_files = self.num_files as i64;

            let indices = {
                let mut self_indices = self.indices;
                let mut additions: Vec<_> = indices
                    .iter()
                    .map(|idx| if *idx < 0 { num_files + *idx } else { *idx } as usize)
                    .collect();
                self_indices.append(&mut additions);
                Self::make_unique(self_indices)
            };
            Ok(Self { indices, ..self })
        } else {
            Ok(self)
        }
    }

    /// Select all files that have the same hash as the names given
    pub fn by_name(self, names: &[&str], prefix_length: u8) -> Result<Self> {
        if names.len() > 0 {
            let indices = {
                let mut indices = self.indices;

                let hash_to_file: HashMap<String, usize> = self
                    .all_files
                    .iter()
                    .filter(|(_, path)| path.parent().is_some())
                    .map(|(idx, path)| {
                        let prefix = path.parent().unwrap();
                        let truncated_prefix = prefix
                            .to_string_lossy()
                            .chars()
                            .take(prefix_length as usize)
                            .collect();
                        (truncated_prefix, *idx)
                    })
                    .collect();

                for file in names {
                    let hash = util::get_hash(Path::new(file), prefix_length)?;
                    match hash_to_file.get(&hash) {
                        Some(idx) => indices.push(*idx),
                        None => bail!("No file with same hash found on server: {}", file),
                    }
                }
                Self::make_unique(indices)
            };
            Ok(Self { indices, ..self })
        } else {
            Ok(self)
        }
    }

    pub fn iter(&'a self) -> Result<FileListingIter<'a>> {
        let stats = self.stats.as_ref();
        let paths = &self.all_files;
        Ok(FileListingIter::new(&self.indices[..], paths, stats))
    }

    /// Only use last `n` files
    pub fn last(self, n: Option<usize>) -> Self {
        match n {
            Some(n) => {
                let num_indices = self.indices.len();
                let indices = self
                    .indices
                    .into_iter()
                    .skip(if num_indices > n { num_indices - n } else { 0 })
                    .collect();
                Self { indices, ..self }
            }
            None => self,
        }
    }

    pub fn revert(mut self, do_revert: bool) -> Self {
        if do_revert {
            self.indices.reverse();
        }
        self
    }

    pub fn sort_by_size(mut self, sort_by_size: bool) -> Result<Self> {
        if sort_by_size {
            self.ensure_stats()?;
            let stats = self.stats.as_ref().unwrap();
            self.indices
                .sort_by_key(|idx| stats.get(idx).unwrap().size.unwrap());
        }
        Ok(self)
    }

    pub fn sort_by_time(mut self, sort_by_time: bool) -> Result<Self> {
        if sort_by_time {
            self.ensure_stats()?;
            let stats = self.stats.as_ref().unwrap();
            self.indices
                .sort_by_key(|idx| stats.get(idx).unwrap().mtime.unwrap());
        }
        Ok(self)
    }

    /// Simply select all files if argument is true
    pub fn with_all(mut self, select_all: bool) -> Self {
        if select_all {
            let mut all: Vec<usize> = (0..self.num_files).collect();
            self.indices.append(&mut all);
            self.indices = Self::make_unique(self.indices.drain(..));
        }
        self
    }

    /// Add all if, so far, no files have been selected
    pub fn with_all_if_none(self) -> Self {
        if self.indices.len() == 0 {
            self.with_all(true)
        } else {
            self
        }
    }

    pub fn with_stats(mut self, with_stats: bool) -> Result<Self> {
        if with_stats {
            self.ensure_stats()?;
        }
        Ok(self)
    }

    fn ensure_stats(&mut self) -> Result<()> {
        if self.stats.is_none() {
            let paths = self
                .indices
                .iter()
                .map(|i| self.all_files.get(i).unwrap().as_path());
            let idx = self.indices.iter().map(|idx| *idx);
            let raw_stats = self.ssh.stat(paths)?;
            self.stats = Some(idx.zip(raw_stats.into_iter()).collect());
        }
        Ok(())
    }

    fn make_unique<I: IntoIterator<Item = usize>>(indices: I) -> Vec<usize> {
        indices.into_iter().unique().collect()
    }
}

pub struct FileListingIter<'a> {
    iter_idx: std::slice::Iter<'a, usize>,
    files: &'a HashMap<usize, PathBuf>,
    stats: Option<&'a HashMap<usize, FileStat>>,
}

impl<'a> FileListingIter<'a> {
    fn new(
        indices: &'a [usize],
        files: &'a HashMap<usize, PathBuf>,
        stats: Option<&'a HashMap<usize, FileStat>>,
    ) -> Self {
        Self {
            iter_idx: indices.iter(),
            files,
            stats,
        }
    }
}

impl<'a> Iterator for FileListingIter<'a> {
    type Item = (usize, &'a Path, Option<&'a FileStat>);

    fn next(&mut self) -> Option<Self::Item> {
        let (idx, file) = match self.iter_idx.next().cloned() {
            Some(idx) => (idx, self.files.get(&idx).unwrap()),
            None => return None,
        };
        let stat = self.stats.map(|s| s.get(&idx).unwrap());

        Some((idx, file, stat))
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
            if username.len() > 0 {
                format!(" for {}", username)
            } else {
                "".to_string()
            }
        );
        if instructions.len() > 0 {
            info!("{}", instructions);
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
                    prompt_password_stderr(&p.text).unwrap_or_default()
                }
            })
            .collect()
    }
}
