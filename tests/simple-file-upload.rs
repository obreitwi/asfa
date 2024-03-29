use anyhow::{bail, Context, Result};
use cmd_lib_core::{run_cmd, run_fun};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod fixture;
use fixture::cargo_run;

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
    cargo_run(
        host,
        &format!("push {} --alias {} --expire none", local.display(), alias),
    )
    .with_context(|| "Could not push.")?;
    cargo_run(
        host,
        &format!("push {} --alias {} --expire none", local.display(), alias),
    )
    .with_context(|| "Second push of identical file failed.")?;
    cargo_run(host, "verify").with_context(|| "Could not verify.")?;
    cargo_run(host, &format!("check {}", local.display())).with_context(|| "Could not check.")?;
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
    cargo_run(host, "verify").with_context(|| "Could not verify.")?;
    cargo_run(
        host,
        &format!("clean --file {} --no-confirm", local.display()),
    )
    .with_context(|| "Could not clean.")?;
    if Path::new(&remote).exists() {
        bail!("Remote file not cleaned up!");
    }
    fs::remove_file(local)?;

    Ok(())
}

fn upload_with_prefix_suffix(host: &str) -> Result<()> {
    let file_size: usize = 32 * 1024 * 1024;
    log::info!("Uploading to host: {}", host);

    let len_local = 12;
    let filename = fixture::random_filename(len_local, "txt");
    let local = fixture::make_random_file(&filename, file_size)?;
    let prefix = fixture::random_string(4);
    let suffix = fixture::random_string(4);

    let hash = run_fun(format!("sha256sum {}", local.display()))?
        .split_whitespace()
        .next()
        .with_context(|| "Could not compute hash")?
        .to_string();

    let hash_b64 = base64::encode_config(hex::decode(hash)?, base64::URL_SAFE);
    run_cmd(format!(
        "cargo run -- --loglevel debug -H {} push {} --prefix {}_ --suffix _{}",
        host,
        local.display(),
        prefix,
        suffix
    ))
    .with_context(|| "Could not push.")?;
    run_cmd(format!("cargo run -- --loglevel debug -H {} verify", host,))
        .with_context(|| "Could not verify.")?;
    let remote = format!(
        "{}/{}/{}_{}_{}.txt",
        std::env::var("ASFA_FOLDER_UPLOAD")?,
        &hash_b64[..32], // TODO: right now prefix length in ci-config is set to 32 -> read from config
        prefix,
        filename.chars().take(len_local).collect::<String>(),
        suffix
    );
    if !Path::new(&remote).exists() {
        bail!("Failed to upload path: {} not found.", remote);
    }
    run_cmd(format!("diff -q \"{}\" \"{}\"", local.display(), remote,))
        .with_context(|| "Files differ")?;
    run_cmd(format!("cargo run -- --loglevel debug -H {} verify", host,))
        .with_context(|| "Could not verify.")?;
    cargo_run(
        host,
        &format!("clean --file {} --no-confirm", local.display()),
    )
    .with_context(|| "Could not clean.")?;
    if Path::new(&remote).exists() {
        bail!("Remote file not cleaned up!");
    }
    fs::remove_file(local)?;

    Ok(())
}

fn expiring_file_upload_begin(host: &str) -> Result<(PathBuf, Instant)> {
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
    // also test specifying alias for single file first
    cargo_run(host, &format!("push --alias {} {}", alias, local.display()))
        .with_context(|| "Could not push.")?;
    cargo_run(host, "verify").with_context(|| "Could not verify.")?;
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
    cargo_run(host, "verify").with_context(|| "Could not verify.")?;
    fs::remove_file(local)?;

    Ok((PathBuf::from(remote), Instant::now()))
}

fn expiring_file_upload_verify(path: &Path, uploaded_at: &Instant) -> Result<()> {
    let need_to_wait = Duration::from_secs(121); // wait two minutes to be sure
    let mut already_waited = Instant::now().duration_since(*uploaded_at);
    while Path::new(path).exists() && already_waited < need_to_wait {
        println!(
            "Waiting {}s for upload to expire.",
            already_waited.as_secs()
        );
        std::thread::sleep(std::cmp::min(
            need_to_wait - already_waited,
            std::time::Duration::from_secs(10),
        ));
        already_waited = Instant::now().duration_since(*uploaded_at);
    }

    if Path::new(path).exists() {
        bail!("Remote file that should have expired was not cleaned up!");
    }

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
    cargo_run(
        host,
        &format!("push {} --alias {} {}", local.display(), alias, arg_limit),
    )
    .with_context(|| "Could not push.")?;
    let finish = Instant::now();
    if finish.duration_since(start) < expected_min_duration {
        bail!(
            "Expected upload to take at least {}s, took: {}s -> Speed limit not applied!",
            finish.duration_since(start).as_secs(),
            expected_min_duration.as_secs()
        );
    }

    cargo_run(host, "verify").with_context(|| "Could not verify.")?;
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

fn clean_all(host: &str) -> Result<()> {
    if run_cmd(format!("cargo run -- --loglevel debug -H {} list", host,)).is_ok() {
        run_cmd(format!(
            "cargo run -- --loglevel debug -H {} clean --all --no-confirm",
            host,
        ))
        .with_context(|| "Could not clean.")?;
    }
    Ok(())
}

#[test]
fn run_simple_uploads() -> Result<()> {
    fixture::testing_prelude()?;

    let (upload_to_expire, to_expire_uploaded_at) =
        expiring_file_upload_begin("asfa-ci-pw-expire")?;
    simple_file_upload("asfa-ci-pw")?;
    simple_file_upload("asfa-ci-key")?;
    upload_with_prefix_suffix("asfa-ci-key")?;
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
    expiring_file_upload_verify(&upload_to_expire, &to_expire_uploaded_at)?;
    clean_all("asfa-ci-key")?;

    Ok(())
}
