<p align="center">
<img src="https://raw.github.com/obreitwi/asfa/master/img/logo.svg" height="96">
</p>

# `asfa` - avoid sending file attachments

[![Crates.io](https://img.shields.io/crates/v/asfa)](https://crates.io/crates/asfa)
[![AUR version](https://img.shields.io/aur/version/asfa)](https://aur.archlinux.org/packages/asfa/)
[![AUR version](https://img.shields.io/aur/version/asfa-git)](https://aur.archlinux.org/packages/asfa-git/)
[![Changelog](https://img.shields.io/badge/changelog-asfa-yellow)](https://github.com/obreitwi/asfa/blob/master/CHANGELOG.md)
[![GitHub commits since tagged version](https://img.shields.io/github/commits-since/obreitwi/asfa/v0.7.5)](https://www.github.com/obreitwi/asfa)
<br />
[![GitHub Workflow Status](https://img.shields.io/github/workflow/status/obreitwi/asfa/cargo%20test)](https://github.com/obreitwi/asfa/actions?query=workflow%3A%22cargo+test%22)
[![dependency status](https://deps.rs/repo/github/obreitwi/asfa/status.svg)](https://deps.rs/repo/github/obreitwi/asfa)
[![Rustdoc](https://img.shields.io/badge/docs-rustdoc-blue)](https://obreitwi.github.io/asfa/)
[![Crates.io](https://img.shields.io/crates/l/asfa)](#license)

![][gif-send]

Instead of email attachments or direct file transfers, upload files from the command line to your web server via `ssh` and send the link instead.
The link prefix is generated from the uploaded file's checksum.
Hence, only people with the correct link can access it.

Comes with a few convenience features:

* Has support to expire links after a set amount of time.
* The link "just works" for non-tech-savvy people, but still only accessible for people who possess the link.
* Does not require any custom binary to be executed to the web server.
* Optional server-side dependencies are readily available ([`at`][at], [`sha2`][sha2]).
* Easily [keep track](#list) of which files are shared currently.
* [Clean](#clean) files by index, checksum or [age](#filtering-by-upload-date).
* After upload files are [verified](#verify) (optionally).
* Supports [aliases](#push-with-alias) at upload because sometimes `plot_with_specific_parameters.svg` is more descriptive than `plot.svg`, especially a few weeks later.
* And _most importantly_, of course: Have a name that can be typed with the left hand on home row only.

`asfa` uses a single `ssh`-connection for each invocation which is convenient if you have [confirmations enabled][gpg-agent-confirm] for each ssh-agent usage (see [details](#background)).
Alternatively, private key files in OpenSSH or [PEM-format][pem] can be used directly.
Even though they should not, plain passwords are accepted as well.

## Requirements

A remote server that
* is accessible via ssh
* has a web server running
* has a folder by your user that is served by your web server
* _(optional)_ has `sha2`-related hashing tools installed (`sha256sum`/`sha512sum`)
* _(optional)_ has [`at`][at] installed to support expiring links.

## Usage

Note: All commands can be abbreviated:
* `p` → `push`
* `l` → `list`
* `ch` → `check`
* `cl` → `clean`
* `v` → `verify`

#### Push

Push (upload) a local file to the remote site and print the URL under which it is reachable.
```text
$ asfa push my-file.txt
https://my-domain.eu/asfa/V66lLtli0Ei4hw3tNkCTXOcweBrneNjt/my-very-specific-file.txt
```
See example at the top. Because the file is identified by its hash, uploading the same file twice will generate the same link.

#### Push with alias

Push a file to the server under a different name. This is useful if you want to share a logfile or plot with a generic name.

![][gif-alias-01]

Note that if you specify several files to upload with their own aliases, you need to explicity assign the arguments.

![][gif-alias-02]

Or specify the aliases afterwards.
```text
$ asfa push my-file.txt my-file-2.txt --alias my-very-specific-file.txt my-very-specific-file-2.txt
https://my-domain.eu/asfa/V66lLtli0Ei4hw3tNkCTXOcweBrneNjt/my-very-specific-file.txt
https://my-domain.eu/asfa/HiGdwtoXcXotyhDxQxydu4zqKwFQ-9pY/my-very-specific-file-2.txt
```

#### Automatic Expire

Uploads can be automatically expired after a certain time via `--expire <delay>`.
`<delay>` can be anything from minutes to hours, days or even months.
It requires [`at`][at] to be installed and running at the remote site.

#### List

List all files currently available online:

![][gif-list]

#### Detailed list

List all files with meta data via `--details`:

![][gif-list-details]

#### Check

Check if files have already been uploaded (via hash) and print them.

#### Clean

Remove the file from remote site via index (negative indices need to be sepearated by `--`):

![][gif-clean]
![][gif-clean-signed]

You can also ensure that a specific file is deleted by specifying `--file`:

![][gif-clean-checksum]

Note that the file is deleted even though it was uploaded with an alias.

#### Verify

In case an upload gets canceled early, all files can be checked for validity via `verify`:

```text
$ asfa verify
✓ my-very-specific-file.txt ... Verified.
✓ my-very-specific-file-2.txt . Verified.
```

Since the prefix is the checksum, the check can be performed whether the file exists locally or not.

#### Filtering by upload date

All commands accept a `--newer`/`--older` `<n>{min,hour,day,week,month}`
argument that can be used to narrow down the number of files.

Cleaning all files older than a month can, for example, be achieved via
```text
$ asfa clean --older 1month
$ asfa clean --older 1M
```

All files uploaded within the last five minutes can be listed via
```text
$ asfa list --newer 5min
$ asfa list --newer 5m
```

## Install

### `cargo`
```text
$ cargo install asfa
```

### AUR
AUR packages [`asfa`][aur-asfa]/[`asfa-git`][aur-asfa-git] provide the latest version/commit from `master`.

Either use your favorite AUR helper or install manually:
```text
$ cd <temporary folder>
$ curl -o PKGBUILD https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h=asfa-git
$ makepkg
[...]
==> Finished making: asfa-git 0.7.2.r16.g763f726-1 (Sun 07 Feb 2021 04:18:12 PM CET)
$ sudo pacman -U asfa-git-0.7.2.r16.g763f726-1-x86_64.pkg.tar.zst
```

### From source
```text
$ git clone https://github.com/obreitwi/asfa.git
$ cargo install --path asfa
```

## Configuration

Configuration resides in `~/.config/asfa/config.yaml`.
Host-specific configuration can also be split into single files residing under `~/.config/asfa/hosts/<alias>.yaml`.

System-wide configuration can be placed in `/etc/asfa` with the same folder structure.

An example config can be found in `./example-config`.
Here, we assume that your server can be reached at `https://my-domain.eu` and that the folder `/var/wwww/default/asfa` will be served at `https://my-domain.eu/asfa`.

### `asfa`-side

A fully commented example config can be found [here](example-config/asfa).

#### Minimal: `~/.config/asfa/hosts/my-remote-site.yaml`

```yaml
hostname: my-hostname.eu  # if not specified, will defaulted from ssh or filename
folder: /var/www/default/asfa
url: https://my-domain.eu/asfa
group: www-data
```

#### Full (single-file): `~/.config/asfa/config.yaml`

```yaml
default_host: my-remote-site
details: true  # optional, acts as if --details is given
prefix_length: 32
verify_via_hash: true
auth:
  interactive: true
  use_agent: true
hosts:
  my-remote-site:
    # note: port is optional, will be inferred form ssh and defaults to 22
    hostname: my-hostname.eu:22
    folder: /var/www/default/asfa
    url: https://my-domain.eu/asfa
    group: www-data
    auth:
      interactive: false
      use_agent: true
      private_key_file: /path/to/private/key/in/pem/format #optional
```

### Web Server

Whatever web server you are using, you have to make sure the following requirements are met:
* The user as which you upload needs to have write access to your configured `folder`.
* Your web server needs to serve `folder` at `url`.
* In case you do not want your uploaded data to be world-readable, set `group` to the group of your web server.
* Make sure your web server does not serve indexes of `folder`, otherwise any visitor can see all uploaded files rather easily.

#### Apache

Your apache config can be as simple as:
```apache
<Directory /var/www/default/asfa>
  Options None
  allow from all
</Directory>
```
Make sure that Options does not contain `Indexes`, otherwise any visitor could _very_ easily access all uploaded files!

#### nginx

```nginx
location /asfa {
  autoindex off
}
```

## Background

Since I handle my emails mostly via ssh on a remote server (shoutout to [neomutt][], [OfflineIMAP][offlineimap] and [msmtp][]), I needed a quick and easy possibility to attach files to emails.
As email attachments are rightfully frowned upon, I did not want to simply copy files over to the remote site to attach them.
Furthermore, I often need to share generated files (such as plots or logfiles) on our group-internal [mattermost](https://www.mattermost.org) or any other form of text-based communication.
Ideally, I want to do this from the folder I am already in on the terminal - and not by to navigating back to it from the browser's "file open" menu…

As a small exercise for writing rust (other than [Advent of Code](https://adventofcode.com/)), I ported a small [python script][py-rpush] I had been using for a couple of years.

For [security reasons][ssh-agent-hijacking] I have my `gpg-agent` (acting as `ssh-agent`) set up to [confirm][gpg-agent-confirm] each usage upon connecting to remote servers and the previous hack required three connections (and confirmations) to perform its task.
`asfa` is set up to only use one ssh-connection per invocation.

## License

Licensed under either of
 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

[at]: https://pubs.opengroup.org/onlinepubs/9699919799/utilities/at.html
[aur-asfa-git]: https://aur.archlinux.org/packages/asfa/
[aur-asfa]: https://aur.archlinux.org/packages/asfa/
[gif-alias-01]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/push_alias_01.gif
[gif-alias-02]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/push_alias_02.gif
[gif-aliases]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/push_alias_02.gif
[gif-clean-checksum]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/clean_02.gif
[gif-clean-signed]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/clean_03.gif
[gif-clean]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/clean_01.gif
[gif-list-details]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/list_details_01.gif
[gif-list]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/list_01.gif
[gif-send]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/push_single_01.gif
[gpg-agent-confirm]: https://www.gnupg.org/documentation/manuals/gnupg/Agent-Configuration.html#index-sshcontrol
[msmtp]: https://marlam.de/msmtp/
[neomutt]: https://neomutt.org/
[offlineimap]: http://www.offlineimap.org/
[pem]: https://serverfault.com/a/706342
[py-rpush]: https://github.com/obreitwi/py-rpush
[sha2]: https://linux.die.net/man/1/sha256sum
[ssh-agent-hijacking]: https://www.clockwork.com/news/2012/09/28/602/ssh_agent_hijacking/
