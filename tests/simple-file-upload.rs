use anyhow::{Context, Result};
use cmd_lib_core::{run_cmd, run_fun};

mod fixture;

fn simple_file_upload(host: &str) -> Result<()> {
    let local = fixture::random_filename(12, "txt");
    let alias = fixture::random_filename(8, "txt");

    let file_size: usize = 32 * 1024 * 1024;
    fixture::make_random_file(&local, file_size)?;
    // TODO: right now prefix length in ci-config is set to 32 -> read from config
    let hash = run_fun(format!("sha256sum {}", local))?
        .split_whitespace()
        .next()
        .with_context(|| "Could not compute hash")?[..32]
        .to_string();
    run_cmd(format!(
        "cargo run -- -H {} push {} --alias {}",
        host, local, alias
    ))?;
    run_cmd(format!(
        "diff -q {} \"{}/{}/{}\"",
        local,
        std::env::var("ASFA_FOLDER_UPLOAD")?,
        hash,
        alias
    ))?;
    Ok(())
}

#[test]
fn run_tests() -> Result<()> {
    fixture::ensure_env()?;

    simple_file_upload("asfa-ci-pw")?;
    Ok(())
}
