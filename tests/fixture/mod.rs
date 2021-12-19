#![allow(dead_code)]

use anyhow::{bail, Context, Result};
use cmd_lib_core::{run_cmd, run_fun};
use lazy_static::lazy_static;
use rand::prelude::*;
use simple_logger::SimpleLogger;
use std::path::{Path, PathBuf};
use std::process::Command;

lazy_static! {
    static ref IS_SET_UP: bool = run_cmd("docker container exec asfa-ci hostname").is_ok();
    static ref TEST_ROOT: PathBuf = PathBuf::from(std::env::var("ASFA_TEST_ROOT").unwrap());
}

pub fn testing_prelude() -> Result<()> {
    setup_logger()?;
    ensure_env()?;
    Ok(())
}

pub fn setup_logger() -> Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()?;
    Ok(())
}

pub fn ensure_env() -> Result<()> {
    if !(*IS_SET_UP) {
        bail!("CI environment is not set up!");
    }
    Ok(())
}

/// Prepare asfa command execution
pub fn prepare_cmd(host: &str) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(&["run", "--", "--loglevel", "debug", "-H", host]);
    cmd
}

fn get_prefix(host: &str) -> String {
    format!("cargo run -- --loglevel debug -H {}", host)
}

/// Wrapper around run_cmd from cmd_lib_core
pub fn cargo_run(host: &str, args: &str) -> std::io::Result<()> {
    run_cmd(format!("{} {}", get_prefix(host), args))
}

/// Wrapper around run_fun from cmd_lib_core
pub fn cargo_run_fun(host: &str, args: &str) -> std::io::Result<String> {
    run_fun(format!("{} {}", get_prefix(host), args))
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

/// Get the expected remote path of a given local file.
pub fn get_remote_path(local: &Path) -> Result<PathBuf> {
    let hash = run_fun(format!("sha256sum {}", local.display()))?
        .split_whitespace()
        .next()
        .with_context(|| "Could not compute hash")?
        .to_string();

    let hash_b64 = base64::encode_config(hex::decode(hash)?, base64::URL_SAFE);
    let mut pb = PathBuf::new();
    pb.push(
        std::env::var("ASFA_FOLDER_UPLOAD")
            .with_context(|| "Could not get remote upload folder from env.")?,
    );
    pb.push(&hash_b64[..32]); // TODO: right now prefix length in ci-config is set to 32 -> read from config
    pb.push(
        local
            .file_name()
            .with_context(|| "Supplied file has no file name.")?,
    );
    Ok(pb)
}
