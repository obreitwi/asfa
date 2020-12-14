use anyhow::{bail, Context, Result};
use cmd_lib_core::{run_cmd, run_fun};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

mod fixture;

fn simple_file_upload(host: &str) -> Result<()> {
    let file_size: usize = 32 * 1024 * 1024;
    log::info!("Uploading to host: {}", host);

    let local = fixture::make_random_file(fixture::random_filename(12, "txt"), file_size)?;
    let alias = fixture::random_filename(8, "txt");

    let hash = run_fun(format!("sha256sum {}", local.display()))?
        .split_whitespace()
        .next()
        .with_context(|| "Could not compute hash")?
        .to_string();

    let hash_b64 = base64::encode_config(hex::decode(hash)?, base64::URL_SAFE);
    run_cmd(format!(
        "cargo run -- --loglevel debug -H {} push {} --alias {}",
        host,
        local.display(),
        alias
    ))
    .with_context(|| "Could not push.")?;
    run_cmd(format!("cargo run -- --loglevel debug -H {} verify", host,))
        .with_context(|| "Could not verify.")?;
    let remote = format!(
        "{}/{}/{}",
        std::env::var("ASFA_FOLDER_UPLOAD")?,
        &hash_b64[..32], // TODO: right now prefix length in ci-config is set to 32 -> read from config
        alias
    );
    if !Path::new(&remote).exists() {
        bail!("Failed to upload path.");
    }
    run_cmd(format!("diff -q \"{}\" \"{}\"", local.display(), remote,))
        .with_context(|| "Files differ")?;
    run_cmd(format!("cargo run -- --loglevel debug -H {} verify", host,))
        .with_context(|| "Could not verify.")?;
    run_cmd(format!(
        "cargo run -- --loglevel debug -H {} clean --file {} --no-confirm",
        host,
        local.display()
    ))
    .with_context(|| "Could not clean.")?;
    if Path::new(&remote).exists() {
        bail!("Remote file not cleaned up!");
    }
    fs::remove_file(local)?;

    Ok(())
}

fn simple_file_upload_speed_limited(
    host: &str,
    file_size: usize,
    arg_limit: &str,
    expected_min_duration: Duration,
) -> Result<()> {
    log::info!("Uploading to host: {}", host);

    let local = fixture::make_random_file(fixture::random_filename(12, "txt"), file_size)?;
    let alias = fixture::random_filename(8, "txt");

    let hash = run_fun(format!("sha256sum {}", local.display()))?
        .split_whitespace()
        .next()
        .with_context(|| "Could not compute hash")?
        .to_string();

    let hash_b64 = base64::encode_config(hex::decode(hash)?, base64::URL_SAFE);
    let start = Instant::now();
    run_cmd(format!(
        "cargo run -- --loglevel debug -H {} push {} --alias {} {}",
        host,
        local.display(),
        alias,
        arg_limit
    ))
    .with_context(|| "Could not push.")?;
    let finish = Instant::now();
    if finish.duration_since(start) < expected_min_duration {
        bail!(
            "Expected upload to take at least {}s, took: {}s -> Speed limit not applied!",
            finish.duration_since(start).as_secs(),
            expected_min_duration.as_secs()
        );
    }

    run_cmd(format!("cargo run -- --loglevel debug -H {} verify", host,))
        .with_context(|| "Could not verify.")?;
    let remote = format!(
        "{}/{}/{}",
        std::env::var("ASFA_FOLDER_UPLOAD")?,
        &hash_b64[..32], // TODO: right now prefix length in ci-config is set to 32 -> read from config
        alias
    );
    if !Path::new(&remote).exists() {
        bail!("Failed to upload path.");
    }
    run_cmd(format!("diff -q \"{}\" \"{}\"", local.display(), remote,))
        .with_context(|| "Files differ")?;
    run_cmd(format!("cargo run -- --loglevel debug -H {} verify", host,))
        .with_context(|| "Could not verify.")?;
    run_cmd(format!(
        "cargo run -- --loglevel debug -H {} clean --file {} --no-confirm",
        host,
        local.display()
    ))
    .with_context(|| "Could not clean.")?;
    if Path::new(&remote).exists() {
        bail!("Remote file not cleaned up!");
    }
    fs::remove_file(local)?;

    Ok(())
}

#[test]
fn run_tests() -> Result<()> {
    fixture::ensure_env()?;

    simple_file_upload("asfa-ci-pw")?;
    simple_file_upload("asfa-ci-key")?;
    simple_file_upload_speed_limited(
        "asfa-ci-pw",
        32 * 1024, /* = 32 Kbyte */
        "--limit-kbytes 4.1",
        Duration::from_secs(7),
    )?;
    simple_file_upload_speed_limited(
        "asfa-ci-key",
        1 * 1024 * 1024 / 8, /* = 1 Mbit */
        "--limit-mbits 0.1",
        Duration::from_secs(9),
    )?;
    Ok(())
}
