use anyhow::{bail, Result};
use std::fs;

mod fixture;
use fixture::prepare_cmd;

#[test]
fn ensure_renaming() -> Result<()> {
    fixture::testing_prelude()?;

    let host = "asfa-ci-pw";
    let local = fixture::make_random_file(fixture::random_filename(12, "txt"), 256)?;

    let upload = prepare_cmd(host)
        .args(&["push", &local.to_string_lossy()])
        .spawn()?
        .wait()?;

    if !upload.success() {
        bail!("Couldn't push remote file.")
    }

    let rename_to = "foobar";
    let rename = prepare_cmd(host)
        .args(&["rename", &local.to_string_lossy(), rename_to])
        .spawn()?
        .wait()?;
    if !rename.success() {
        bail!("Rename operation failed.");
    }
    let mut remote = fixture::get_remote_path(&local)?;
    remote.set_file_name(rename_to);
    if !remote.exists() {
        bail!(
            "Rename operation failed: {} does not exist.",
            remote.display()
        );
    }

    let rename_to = "barfoo";
    let rename = prepare_cmd(host)
        .args(&["mv", "-1", rename_to])
        .spawn()?
        .wait()?;
    if !rename.success() {
        bail!("Rename operation #2 failed.");
    }
    remote.set_file_name(rename_to);
    if !remote.exists() {
        bail!(
            "Rename operation failed: {} does not exist.",
            remote.display()
        );
    }

    fs::remove_file(local)?;

    Ok(())
}
