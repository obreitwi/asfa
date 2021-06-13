use anyhow::{bail, Result};
use cmd_lib_core::run_cmd;
use lazy_static::lazy_static;
use rand::prelude::*;
use std::path::{Path, PathBuf};

lazy_static! {
    static ref IS_SET_UP: bool = run_cmd("docker container exec asfa-ci hostname").is_ok();
    static ref TEST_ROOT: PathBuf = PathBuf::from(std::env::var("ASFA_TEST_ROOT").unwrap());
}

pub fn ensure_env() -> Result<()> {
    if !(*IS_SET_UP) {
        bail!("CI environment is not set up!");
    }
    Ok(())
}

/// Generate file with random data - if path is not absolute, file will be created in
/// ASFA_TEST_ROOT.
///
/// Returns absolute path to created file.
pub fn make_random_file<P: AsRef<Path>>(path: P, size: usize) -> Result<PathBuf> {
    let path: PathBuf = if path.as_ref().is_absolute() {
        path.as_ref().to_owned()
    } else {
        let mut fullpath = PathBuf::new();
        fullpath.push(test_root());
        fullpath.push(path.as_ref());
        fullpath
    };

    run_cmd(format!(
        "dd if=/dev/urandom of={} count=1 bs={}",
        path.display(),
        size
    ))?;
    Ok(path)
}

/// Generate random filename of size `len` with specified extension
pub fn random_filename(len: usize, extension: &str) -> String {
    format!("{}.{}", random_string(len), extension)
}

/// Generate random string of size `len`
pub fn random_string(len: usize) -> String {
    let mut rng = rand::thread_rng();
    let chars: Vec<_> = std::iter::repeat(())
        .map(|()| rng.sample(rand::distributions::Alphanumeric))
        .take(len)
        .collect();
    std::str::from_utf8(&chars[..]).unwrap().to_string()
}

/// Get root folder where temporary test files should be placed
pub fn test_root() -> &'static Path {
    &TEST_ROOT
}
