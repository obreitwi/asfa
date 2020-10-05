<p align="center">
<img src="https://raw.github.com/obreitwi/asfa/master/img/logo.svg" height="96">
</p>

# `asfa` - avoid sending file attachments

[![Crates.io](https://img.shields.io/crates/v/asfa)](https://crates.io/crates/asfa)
[![GitHub commits since tagged version](https://img.shields.io/github/commits-since/obreitwi/asfa/v0.3.1)](https://www.github.com/obreitwi/asfa)
[![Build Status](https://travis-ci.com/obreitwi/asfa.svg?branch=master)](https://travis-ci.com/obreitwi/asfa)
[![Rustdoc](https://img.shields.io/badge/docs-rustdoc-blue)](https://obreitwi.github.io/asfa)
[![Crates.io](https://img.shields.io/crates/l/asfa)](#license)

```text
$ asfa push my-file.txt
https://my-domain.eu/my-uploads/999b7625/my-file.txt
```

* Upload files via `ssh` to a (linux-based) remote site.
* Generate a non-guessable URL pointing to the file.
* The URL can then be sent via mail or directly.

`asfa` uses a single `ssh`-connection for each invocation which is convenient
if you have [confirmations enabled][gpg-agent-confirm] for each ssh-agent usage
(see [details](#background)). Alternatively, private key files in [PEM
format][pem] can be used directly.

[gpg-agent-confirm]: https://www.gnupg.org/documentation/manuals/gnupg/Agent-Configuration.html#index-sshcontrol
[pem]: https://serverfault.com/a/706342

## Usage

Note: All commands can actually be abbreviated:
* `p` for `push`
* `l` for `list`
* `c` for `clean`

#### Push

Push (upload) a local file to the remote site and print the URL under which it is reachable:
```text
$ asfa push my-file.txt
https://my-domain.eu/my-uploads/999b7625/my-file.txt
```

#### Push with alias

Push file to server under a different name. This is useful if you want to share
a logfile or plot with a generic name.

```text
$ asfa push my-file.txt -a my-very-specific-file.txt
https://my-domain.eu/my-uploads/999b7625/my-very-specific-file.txt
```

Note that if you specify several files to upload with their own aliases, you need to explicity assign the arguments.
```text
$ asfa push my-file.txt --alias=my-very-specific-file.txt my-file-2.txt --alias=my-very-specific-file-2.txt
https://my-domain.eu/my-uploads/999b7625/my-very-specific-file.txt
https://my-domain.eu/my-uploads/f9d0c87a/my-very-specific-file-2.txt
```

Or specify the aliases afterwards.
```text
$ asfa push my-file.txt my-file-2.txt --alias my-very-specific-file.txt my-very-specific-file-2.txt
https://my-domain.eu/my-uploads/999b7625/my-very-specific-file.txt
https://my-domain.eu/my-uploads/f9d0c87a/my-very-specific-file-2.txt
```

#### List

List all files currently available online:
```text
$ asfa list
2020-09-11 20:13:25,214 INFO  [asfa::cmd::list] Listing remote files:
[0|-2] https://my-domain.eu/my-uploads/999b7625/my-very-specific-file.txt
[1|-1] https://my-domain.eu/my-uploads/f9d0c87a/my-very-specific-file-2.txt
```

#### Clean

Remove the file from remote site via index (negative indices need to be sepearated by `--`):
```text
$ asfa clean 0
[ alternative: $ asfa clean -- -2 ]
2020-09-10 20:14:42,651 INFO  [asfa::ssh] removed '/var/www/default/my-uploads/999b7625/my-file.txt'
2020-09-10 20:14:42,651 INFO  [asfa::ssh] removed directory '/var/www/default/my-uploads/999b7625'
```

You can also ensure that a specific file is deleted by specifying `--file`:
```text
$ asfa clean --file my-file-2.txt
2020-09-10 20:16:29,221 INFO  [asfa::ssh] removed '/var/www/default/my-uploads/f9d0c87a/my-very-specific-file-2.txt'
2020-09-10 20:16:29,221 INFO  [asfa::ssh] removed directory '/var/www/default/my-uploads/f9d0c87a'
```
Note that the file is deleted even though it was uploaded with an alias.

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
