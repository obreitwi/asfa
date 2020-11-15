use crate::ssh::SshSession;
use crate::util;

use anyhow::{bail, Result};
use itertools::Itertools;
use regex::Regex;
use ssh2::FileStat;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
