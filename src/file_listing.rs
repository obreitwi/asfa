use crate::cfg::Host;
use crate::cli::text;
use crate::ssh::SshSession;
use crate::util;

use anyhow::{bail, Context, Result};
use chrono::{Local, TimeZone};
use itertools::Itertools;
use regex::Regex;
use ssh2::FileStat;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Helper structure to avoid re-implementing file listing capabilities for all commands.
pub struct FileListing<'a> {
    pub num_files: usize,
    all_files: HashMap<usize, PathBuf>,
    pub indices: Vec<usize>,
    pub stats: Option<HashMap<usize, FileStat>>,
    ssh: &'a SshSession<'a>,
}

impl<'a> FileListing<'a> {
    pub fn new(ssh: &'a SshSession) -> Result<FileListing<'a>> {
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
        if !indices.is_empty() {
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
    pub fn by_hash<T: AsRef<str>>(
        self,
        names: impl IntoIterator<Item = T>,
        prefix_length: u8,
        bail_when_missing: bool,
    ) -> Result<Self> {
        let mut names = names.into_iter().peekable();
        if names.peek().is_none() {
            Ok(self)
        } else {
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
                    let hash = util::get_hash(Path::new(file.as_ref()), prefix_length)?;
                    match hash_to_file.get(&hash) {
                        Some(idx) => indices.push(*idx),
                        None => {
                            let msg = format!(
                                "No file with same hash found on server: {}",
                                file.as_ref()
                            );
                            if bail_when_missing {
                                bail!("{}", msg);
                            } else {
                                log::warn!("{}", msg);
                            }
                        }
                    }
                }
                Self::make_unique(indices)
            };
            Ok(Self { indices, ..self })
        }
    }

    /// Return count of currently selected files
    pub fn count(&self) -> usize {
        self.indices.len()
    }

    /// Check if file listing has stats
    pub fn has_stats(&self) -> bool {
        self.stats.is_some()
    }

    /// Only use first `n` files
    pub fn first(self, n: Option<usize>) -> Self {
        match n {
            Some(n) => {
                let indices = self.indices.into_iter().take(n).collect();
                Self { indices, ..self }
            }
            None => self,
        }
    }

    pub fn iter(&'a self) -> FileListingIter<'a> {
        let stats = self.stats.as_ref();
        let paths = &self.all_files;
        FileListingIter::new(&self.indices[..], paths, stats)
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

    /// Get formatted lines to be printed with draw_boxed() if stdout is a tty.
    ///
    /// If stdout is no tty, simply format contents separated by tabs for easy parsing.
    ///
    /// If filename_only is specified, prints url if host is specified, otherwise the relative
    /// path.
    pub fn format_files(
        &self,
        host: Option<&Host>,
        filename_only: bool,
        with_size: bool,
        with_time: bool,
    ) -> Result<Vec<String>> {
        let (num_digits, num_digits_rev) = (self.get_num_digits(), self.get_num_digits_rev()?);
        self.iter()
            .map(|(i, file, stat)| -> Result<String> {
                Ok(format!(
                    " {idx:width$}{sep}{rev_idx:rev_width$}{sep}{size}{mtime}{url} ",
                    idx = i,
                    rev_idx = i as i64 - self.num_files as i64,
                    url = if filename_only {
                        file.file_name().unwrap().to_string_lossy().to_string()
                    } else if let Some(host) = host {
                        host.get_url(&format!("{}", file.display()))?
                    } else {
                        file.display().to_string()
                    },
                    width = num_digits,
                    rev_width = num_digits_rev,
                    sep = text::separator(),
                    size = if with_size {
                        stat.as_ref()
                            .map(|s| self.column_size(s))
                            .unwrap_or_else(|| Ok("".to_string()))?
                    } else {
                        "".to_string()
                    },
                    mtime = if with_time {
                        stat.as_ref()
                            .map(|s| self.column_time(s))
                            .unwrap_or_else(|| Ok("".to_string()))?
                    } else {
                        "".to_string()
                    }
                ))
            })
            .collect()
    }

    pub fn revert(mut self, do_revert: bool) -> Self {
        if do_revert {
            self.indices.reverse();
        }
        self
    }

    pub fn select_newer(self, user_duration: Option<&str>) -> Result<Self> {
        self.filter_by_time(user_duration, false)
    }

    pub fn select_older(self, user_duration: Option<&str>) -> Result<Self> {
        self.filter_by_time(user_duration, true)
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

    /// Add all if, so far, no files have been selected and the boolean switch is set.
    pub fn with_all_if_none(self, doit: bool) -> Self {
        if doit && self.indices.is_empty() {
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

    fn column_size(&self, stat: &FileStat) -> Result<String> {
        let possible = ["B", "K", "M", "G", "T", "P", "E"];
        let mut size: u64 = stat.size.with_context(|| "No file size defined!")?;
        for (i, s) in possible.iter().enumerate() {
            // If size is >= 999.5 (which we cannot detect via integer), the printed representation
            // will be rouned to 1000.00 -> move to next higher unit at 999
            if 999 <= size {
                size >>= 10;
                continue;
            } else {
                return Ok(format!(
                    "{size:>6.2}{suffix}{sep}",
                    size = stat.size.unwrap() as f64 / (1 << (i * 10)) as f64,
                    suffix = s,
                    sep = text::separator()
                ));
            }
        }
        bail!("Invalid size argument provided.")
    }

    fn column_time(&self, stat: &FileStat) -> Result<String> {
        let mtime = Local.timestamp(stat.mtime.with_context(|| "File has no mtime.")? as i64, 0);
        Ok(format!(
            "{mtime}{sep}",
            mtime = mtime.format("%Y-%m-%d %H:%M:%S").to_string(),
            sep = text::separator()
        ))
    }

    fn ensure_stats(&mut self) -> Result<()> {
        if self.stats.is_none() {
            let paths = self
                .indices
                .iter()
                .map(|i| self.all_files.get(i).unwrap().as_path());
            let idx = self.indices.iter().copied();
            let raw_stats = self.ssh.stat(paths)?;
            self.stats = Some(idx.zip(raw_stats.into_iter()).collect());
        }
        Ok(())
    }

    fn make_unique<I: IntoIterator<Item = usize>>(indices: I) -> Vec<usize> {
        indices.into_iter().unique().collect()
    }

    /// Get number of digits for index
    fn get_num_digits(&self) -> usize {
        let mut num_digits = 0;
        let mut num = self.num_files;
        while num > 0 {
            num /= 10;
            num_digits += 1;
        }
        num_digits
    }

    /// Get number of digits for reverse index
    fn get_num_digits_rev(&self) -> Result<usize> {
        let mut num_digits = 0;
        let mut num = self.num_files
            - self
                .iter()
                .map(|f| f.0)
                .min()
                .with_context(|| "No files to list.")
                .unwrap_or(0);
        while num > 0 {
            num /= 10;
            num_digits += 1;
        }
        Ok(num_digits + 1) /* minus sign */
    }

    /// Helper function that filters selected files by date.
    ///
    /// If select_older == true, then only files older than user_duration will be kept;
    /// if false, only files newer than user_duration will be kept.
    fn filter_by_time(mut self, user_duration: Option<&str>, select_older: bool) -> Result<Self> {
        if let Some(user_duration) = user_duration {
            let duration = humantime::parse_duration(user_duration)?;
            self.ensure_stats()?;
            let stats = self.stats.as_ref().unwrap();

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards.");
            let cutoff = now
                .checked_sub(duration)
                .context("Invalid duration specified.")?;
            let cutoff_s = cutoff.as_secs();

            let indices: Vec<_> = self
                .indices
                .into_iter()
                .filter(|idx| {
                    let mtime = stats.get(idx).unwrap().mtime.unwrap();
                    if select_older {
                        mtime <= cutoff_s
                    } else {
                        mtime >= cutoff_s
                    }
                })
                .collect();

            Ok(Self { indices, ..self })
        } else {
            Ok(self)
        }
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
