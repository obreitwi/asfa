
# Changelog for [`asfa`](https://github.com/obreitwi/asfa)

## v0.5.5-pre (under development)

* `clean`/`list`/`verify`-commands: Add `--newer`/`--older`-flags
  * They take a duration (seconds, minutes, hours, weeks, months, years) and filter the otherwise selected files
  * [Supported formats](https://docs.rs/humantime/2.0.1/humantime/fn.parse_duration.html): 
    -> tl;dr: Quantity and unit are NOT seperated by spaces
  * Examples:
    * List all files uploaded in the last three days: `asfa list --newer 3weeks`
    * Clean all files older than a month: `asfa clean --older 1month`

## v0.5.4 (2020-11-15)

* Fix host-specific auth settings falling back on the default Auth-settings
  instead of the global Auth-settings defined in `config.yaml`.
* Add support to obtain settings from openSSH.
* If user-supplied settings fail to authenticate against the remote server, try
  using private keys listed in openSSH.
  * This can be toggled via a new `from_openssh`-boolean flag in `auth`-section.
  * For now, this defaults to false - will be changed to true in next
    non-bugfix release.
* Ensure bulk-requested file stats are in correct order.

## v0.5.3 (2020-11-01)

* Rework authentication procedure to only try authentication methods that the server advertises.
* Add support for keyboard-interactive authentication if the user enables interactive input.
  -> Allows for two factor authentication (e.g., with pam-based google-authenticator)
* Fix host settings defined in separate file not falling back to defaults set in `config.yaml`

## v0.5.2 (2020-10-29)

* Improve stat retrieval speed from < 50 entries/s to near instant.
  * Needs `find`, `xargs` and `stat` on the remote site.
  * Falls back to retrieving stats via sftp-interface if needed tools not
    available.

## v0.5.1 (2020-10-23)

* Fix: Remove unused `--no-confirm`-switch from verify command.
* Fix: Colorful output for clean-confirmation.

## v0.5.0 (2020-10-22)

* Allow host selection via `$ASFA_HOST` environment variable. Priority for host selection is:
  1. `-H`/`--host` supplied via command line.
  2. `ASFA_HOST` environment variable.
  3. `default_host` from config file.
  4. Single host if there is only one defined.
* `clean`-command:
  * Add `-F`/`--filter`-option to clean files matching a given regex.
  * Add `-n`/`--last`-switch from `list` command to clean the last `n` files
  * Add `-r`/`--reverse`-switch to reverse listing (useful for `-n`).
  * Add `-S`/`--sort-size`-switch to sort files by remote size (useful for `-n`).
  * Add `-T`/`--sort-time`-switch to sort selected files by modification time.
* `list`-command:
  * Add `-T`/`--sort-time`-switch to sort selected files by modification time.
* Add `verify`-command with same file selection arguments as `clean`/`list`.

## v0.4.1 (2020-10-14)

* Fix name of `prefix_length` in example config.
* `list`: Add `-d`/`--details` switch that simply enables printing of all details.
* Fix formatting error when displaying file sizes that are 1000 {K,M,G,T,…} exactly.

## v0.4.0 (2020-10-14)

* Add `private_key_file` to auth-option in order to specfiy private key file directly
* Add `--verbose` argument that increases loglevel
* Add `--quiet` argument that decreases loglevel
* Encode hash in base64 in order to make needed URL prefix shorter at same "guessability".
  → This causes the maximum prefix length to go down from 128 to 64 (4 bit per char → 8 bit per char)
* `list`:
  * Add `-s`/`--with-size`-switch to print file sizes.
  * Add `-f`/`--filenames`-switch to print filenames instead of full urls in listing.
  * Add `-S`/`--sort-size`-switch to sort selected files by remote size.
  * Add `-r`/`--reverse`-switch to reverse listing.
  * Add `-F`/`--filter`-option to only display filenames matching a given regex.
  * Add `-i`/`--indices`-switch to only print indices of files.
    * This is useful to supply as input to the clean command for instance:
    * Example: `asfa clean $(asfa list -iF "\.png$")` deletes all png.
  * Add `-t`/`--with-time` to print modification times.

## v0.3.1 (2020-10-03)

* Fix prefix length limited to 64 -> now 128
* Fix error when deleting file by hash (via `--file`).

## v0.3.0 (2020-09-27)

* Fix unencoded-URL printed after push (only `list` reported correctly encoded URL)
* Add notifications for upload progress and remote verification
* Add confirmation prior to cleaning (can be avoided by issuing `--no-confirm`)

## v0.2.0 (2020-09-24)

* Add option to specify prefix length

## v0.1.1 (2020-09-22)

* Fix build issue with clap-3.0.0-beta.2

## v0.1.0 (2020-09-14)

* Initial release
