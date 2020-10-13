
# Changelog for [`asfa`](https://github.com/obreitwi/asfa)

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
