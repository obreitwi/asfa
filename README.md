<p align="center">
<img src="https://raw.github.com/obreitwi/asfa/master/img/logo.svg" height="96">
</p>

# `asfa` - avoid sending file attachments

[![Crates.io](https://img.shields.io/crates/v/asfa)](https://crates.io/crates/asfa)
[![GitHub commits since tagged version](https://img.shields.io/github/commits-since/obreitwi/asfa/v0.6.0)](https://www.github.com/obreitwi/asfa)
[![Build Status](https://travis-ci.com/obreitwi/asfa.svg?branch=master)](https://travis-ci.com/obreitwi/asfa)
[![Rustdoc](https://img.shields.io/badge/docs-rustdoc-blue)](https://obreitwi.github.io/asfa)
[![Crates.io](https://img.shields.io/crates/l/asfa)](#license)

![](https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/push_single_01.gif)

Since I handle my emails mostly via ssh on a remote server (shoutout to
[neomutt](https://neomutt.org/), [OfflineIMAP](http://www.offlineimap.org/) and
[msmtp](https://marlam.de/msmtp/)), I needed a quick and easy possibility to
attach files to emails. As email attachments are rightfully frowned upon, I did
not want to simply copy files over to the remote site to attach them.
Furthermore, I often need to share generated files (such as plots or logfiles)
on our group-internal [mattermost](https://www.mattermost.org) or any other
form of text-based communication. Ideally, I want to do this from the folder I
am already in on the terminal - and not by to navigating back to it from the
browser's "file open" menu…

Therefore, I needed a quick tool that let's me

* [send][gif-send] a link instead of the file.
* support [aliases][gif-aliases] because sometimes
  `plot_with_specific_parameters.svg` is more descriptive than `plot.svg`,
  especially a few weeks later.
* have the link "just work" for non-tech-savvy people, i.e. not have the file
  be password-protected, but still only accessible for people who possess the
  link. Here it is helpful to own a domain somewhat resembling your last name.
* [keep][gif-list-details] track of which files I shared.
* easily clean files by [signed][gif-clean-signed] [index][gif-clean], regex or
  [checksum][gif-clean-checksum].
* verify that all files uploaded correctly.
* do everything from the command line.
* have an excuse to to use [Rust](https://www.rust-lang.org/) for something
  other than [Advent of Code](https://adventofcode.com/).
* (have a name that can only be typed with the left hand without moving.)

[gif-send]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/push_single_01.gif
[gif-aliases]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/push_alias_02.gif
[gif-list]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/list_01.gif
[gif-list-details]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/list_details_01.gif
[gif-clean-signed]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/clean_03.gif
[gif-clean]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/clean_01.gif
[gif-clean-checksum]: https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/clean_02.gif

`asfa` works by uploading the given file to a publicly reachable location on
the remote server via SSH. The link prefix of variable length is then generated
from the checksum of the uploaded file. Hence, it is non-guessable (only people
with the correct link can access it) and can be used to verify the file
uploaded correctly.

The emitted link can then be copied and pasted.

`asfa` uses a single `ssh`-connection for each invocation which is convenient
if you have [confirmations enabled][gpg-agent-confirm] for each ssh-agent usage
(see [details](#background)). Alternatively, private key files in [PEM
format][pem] or openssh-format (i.e., private key starts with
`-----BEGIN OPENSSH PRIVATE KEY-----`) can be used directly.

[gpg-agent-confirm]: https://www.gnupg.org/documentation/manuals/gnupg/Agent-Configuration.html#index-sshcontrol
[pem]: https://serverfault.com/a/706342

## Usage

Note: All commands can actually be abbreviated:
* `p` for `push`
* `l` for `list`
* `c` for `clean`
* `v` for `verify`

#### Push

Push (upload) a local file to the remote site and print the URL under which it
is reachable.
```text
$ asfa push my-file.txt
https://my-domain.eu/my-uploads/V66lLtli0Ei4hw3tNkCTXOcweBrneNjt/my-very-specific-file.txt
```
See example at the top. Because the file is identified by its hash, uploading
the same file twice will generate the same link.

#### Push with alias

Push a file to the server under a different name. This is useful if you want to
share a logfile or plot with a generic name.

![](https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/push_alias_01.gif)

Note that if you specify several files to upload with their own aliases, you need to explicity assign the arguments.
![](https://raw.githubusercontent.com/obreitwi/asfa/17b954a6f4aafa03e8f6ef8fcd49f8619c4af7dc/img/push_alias_02.gif)

Or specify the aliases afterwards.
```text
$ asfa push my-file.txt my-file-2.txt --alias my-very-specific-file.txt my-very-specific-file-2.txt
https://my-domain.eu/my-uploads/V66lLtli0Ei4hw3tNkCTXOcweBrneNjt/my-very-specific-file.txt
https://my-domain.eu/my-uploads/HiGdwtoXcXotyhDxQxydu4zqKwFQ-9pY/my-very-specific-file-2.txt
```

#### Automatically expiring uploaded files 

Uploads can be automatically expired after a certain time via `--expire <delay>`.
`<delay>` can be anything from minutes to hours, days even months.
It requires [`at`][at] to be installed and running at the remote site.

[at]: https://pubs.opengroup.org/onlinepubs/9699919799/utilities/at.html

#### List

List all files currently available online:

![][gif-list]

#### Detailed list

List all files with meta data via `--details`:

![][gif-list-details]

#### Clean

Remove the file from remote site via index (negative indices need to be sepearated by `--`):

![][gif-clean]
![][gif-clean-signed]

You can also ensure that a specific file is deleted by specifying `--file`:

![][gif-clean-checksum]

Note that the file is deleted even though it was uploaded with an alias.

### Verify

In case an upload gets canceled early, all files can be checked for validity via `verify`:

```text
$ asfa verify
✓ my-very-specific-file.txt ... Verified.
✓ my-very-specific-file-2.txt . Verified.
```

Since the prefix is the checksum, the check can be performed whether the file exists locally or not.

### Filtering by upload date

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

## Requirements

A remote server that
* is accessible via ssh
* has a webserver running
* has writable folder served by your webserver
* _(optional)_ has `sha2`-related hashing tools installed (`sha256sum`/`sha512sum`)

## Install

Simply install via cargo
```text
$ cargo install asfa
```

or build from source

```text
$ git clone https://github.com/obreitwi/asfa.git
$ cargo install --path asfa
```

## Configuration

Configuration resides in `~/.config/asfa/config.yaml`. Host-specific
configuration can also be split into single files residing under
`~/.config/asfa/hosts/<alias>.yaml`.

System-wide configuration can be placed in `/etc/asfa` with the same folder
structure.

An example config can be found in `./example-config`.
Here, we assume that your server can be reached at `https://my-domain.eu` and
that the folder `/var/wwww/default/my-uploads` will be served at
`https://my-domain.eu/my-uploads`.

### `asfa`-side

A fully commented example config can be found
[here](example-config/asfa).

#### Minimal: `~/.config/asfa/hosts/my-remote-site.yaml`

```yaml
hostname: my-hostname.eu
folder: /var/www/default/my-uploads
url: https://my-domain.eu/my-uploads
group: www-data
```

#### Full (single-file): `~/.config/asfa/config.yaml`

```yaml
default_host: my-remote-site
prefix_length: 32
verify_via_hash: true
auth:
  interactive: true
  use_agent: true
hosts:
  my-remote-site:
    # note: port is optional and defaults to 22
    hostname: my-hostname.eu:22
    folder: /var/www/default/my-uploads
    url: https://my-domain.eu/my-uploads
    group: www-data
    auth:
      interactive: false
      use_agent: true
      private_key_file: /path/to/private/key/in/pem/format #optional
```

### Webserver

Whatever webserver you are using, you have to make sure the following
requirements are met:
* The user as which you upload needs to have write access to your configured
  `folder`.
* Your webserver needs to serve `folder` at `url`.
* In case you do not want your uploaded data to be world-readable, set `group`
  to the group of your webserver.
* Make sure your webserver does not serve indexes of `folder`, otherwise any
  visitor can see all uploaded files rather easily.

#### Apache

Your apache config can be as simple as:
```apache
<Directory /var/www/default/my-uploads>
  Options None
  allow from all
</Directory>
```
Make sure that Options does not contain `Indexes`, otherwise any visitor could
_very_ easily access all uploaded files.

#### nginx

```nginx
location /my-uploads {
  autoindex off
}
```

## Background

As a small exercise for writing rust, I ported a small [python
script][py-rpush] I had been using for a couple of years.

For [security reasons][ssh-agent-hijacking] I have my `gpg-agent` (acting as `ssh-agent`) set up to
[confirm][gpg-agent-confirm] each usage upon connecting to remote servers and
the previous hack required three connections (and confirmations) to perform its
task. `asfa` is set up to only use one ssh-connection per invocation.

[py-rpush]: https://github.com/obreitwi/py-rpush
[ssh-agent-hijacking]: https://www.clockwork.com/news/2012/09/28/602/ssh_agent_hijacking/

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
