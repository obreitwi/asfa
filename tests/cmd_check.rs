use anyhow::{bail, Context, Result};
use std::fs;
use std::process::Command;

mod fixture;

#[test]
fn ensure_return_code_failed_check() -> Result<()> {
    fixture::ensure_env()?;

    let host = "asfa-ci-pw";
    let local = fixture::make_random_file(fixture::random_filename(12, "txt"), 256)?;

    let output = Command::new("cargo")
        .args(&[
            "run",
            "--",
            "--loglevel",
            "debug",
            "-H",
            host,
            "check",
            &local.to_string_lossy(),
        ])
        .output()
        .context("Couldn't execute command")?;

    if !matches!(output.status.code(), Some(1)){
        bail!("Expected return 1, found {:?}", output.status.code());
    }

    fs::remove_file(local)?;

    Ok(())
}
