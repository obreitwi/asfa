use anyhow::{bail, Context, Result};
use expanduser::expanduser;
use log::{debug, warn};
use std::collections::HashMap;
use std::fmt::Display;
use std::fs::{read_dir, read_to_string};
use std::path::PathBuf;
use whoami::username;
use yaml_rust::{yaml::Hash, Yaml, YamlLoader};

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
const FRAGMENT: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');

use crate::util::*;

/// The main configuration
#[derive(Debug)]
pub struct Config {
    /// Authentication settings to use if no host-specific authentication settings specified.
    pub auth: Auth,

    /// Default host to upload to.
    default_host: Option<String>,

    /// List of all configured hosts.
    hosts: HashMap<String, Host>,

    /// Length of prefix to use unless overwritten in host
    pub prefix_length: u8,

    /// Compute hash on remote side after upload to verify.
    pub verify_via_hash: bool,
}

/// Authentication configuration
#[derive(Debug)]
pub struct Auth {
    /// Perform agent authentication
    pub use_agent: bool,

    /// Perform interactive authentication
    pub interactive: bool,
}

/// A host entry
#[derive(Debug)]
pub struct Host {
    /// Alias under which the host is known
    pub alias: String,

    /// Overwrite global authentication settings for this host.
    pub auth: Option<Auth>,

    /// In which folder do we store files on the host.
    pub folder: PathBuf,

    /// In case files on the remote site need to have a special group setting in order to be
    /// readable by the webserver.
    pub group: Option<String>,

    /// Self-explanatory (if not set alias will be used)
    pub hostname: Option<String>,

    /// If the user REALLY REALLY wants to, a plaintext password can be provided (but it is not
    /// recommended!).
    pub password: Option<String>,

    /// Length of prefix to use
    pub prefix_length: Option<u8>,

    /// url-prefix to apply to random link
    pub url: String,

    /// The user to sign in, otherwise ssh config will be used.
    pub user: Option<String>,
}

fn default_config_directories() -> Vec<&'static str> {
    vec!["~/.config/asfa", "/etc/asfa"]
}

pub fn get<T: AsRef<str> + Display>(path: &Option<T>) -> Result<Config> {
    let possible_paths: Vec<&str> = match path {
        Some(path) => vec![path.as_ref()],
        None => default_config_directories(),
    };
    for path in possible_paths.iter() {
        match Config::get(path)? {
            None => continue,
            Some(cfg) => return Ok(cfg),
        }
    }
    bail!("Did not find valid configuration!");
}

#[allow(dead_code)]
pub fn dummy_host_str() -> &'static str {
    include_str!("dummy_host.yml")
}

#[allow(dead_code)]
pub fn dummy_host() -> Host {
    Host::from_yaml(
        "dummy_host".to_string(),
        &YamlLoader::load_from_str(dummy_host_str()).unwrap()[0],
    )
    .unwrap()
}

impl Config {
    pub fn get<T: AsRef<str> + Display>(dir: T) -> Result<Option<Config>> {
        let config_dir = match expanduser(dir.as_ref()) {
            Ok(p) => p,
            Err(e) => {
                bail!("Error when expanding path to config file: {}", e);
            }
        };
        let global = {
            let mut global = config_dir.clone();
            global.push("config.yaml");
            global
        };
        let raw: String = match read_to_string(&global) {
            Err(e) => {
                debug!(
                    "Could not read configuration file '{}', error: {}",
                    global.to_str().unwrap_or("invalid"),
                    e
                );
                return Ok(None);
            }
            Ok(raw) => raw,
        };

        let mut config = Self::from_yaml(&raw)?;

        let hosts_dir = {
            let mut hosts_dir = config_dir.clone();
            hosts_dir.push("hosts");
            hosts_dir
        };

        if hosts_dir.is_dir() {
            for entry in read_dir(&hosts_dir)? {
                let possible_host = entry?.path();
                match possible_host.extension() {
                    None => {
                        continue;
                    }
                    Some(ext) => {
                        if ext != "yaml" {
                            continue;
                        }
                    }
                };
                let alias = match possible_host.file_stem() {
                    None => {
                        warn!(
                            "Could not extract file stem for: {}",
                            possible_host.display()
                        );
                        continue;
                    }
                    Some(alias) => alias
                        .to_str()
                        .context("Could not convert host file name to String.")?
                        .to_string(),
                };
                if config.hosts.contains_key(&alias) {
                    bail!("Host {} configured in config.yaml and as host-file.", alias);
                };

                let host_yaml = YamlLoader::load_from_str(&read_to_string(&possible_host)?)?;
                let error = format!("Invalid host-file for host {}", &alias);
                let host = Host::from_yaml(alias, &host_yaml[0]).context(error)?;

                config.hosts.insert(host.alias.clone(), host);
            }
        }
        Ok(Some(config))
    }

    pub fn from_yaml(input: &str) -> Result<Config> {
        let documents = match YamlLoader::load_from_str(input) {
            Ok(data) => data,
            Err(e) => {
                bail!("Error while loading config file: {}", e);
            }
        };

        let config_yaml = match &documents[0] {
            Yaml::Hash(h) => h,
            _ => {
                bail!("Root object in configuration file is no dictionary!");
            }
        };

        let mut hosts = HashMap::new();
        let prefix_length = {
            let length = get_int_from(config_yaml, "prefix_length")?
                .cloned()
                .unwrap_or(32);
            check_prefix_length(length)?;
            length as u8
        };

        match config_yaml.get(&yaml_string("hosts")) {
            Some(Yaml::Hash(dict)) => {
                for entry in dict.clone().entries() {
                    let alias = match entry.key() {
                        Yaml::String(alias) => alias.to_string(),
                        invalid => {
                            warn!("Found invalid alias for host entry: {:?}", invalid);
                            continue;
                        }
                    };
                    let host_yaml = entry.get();
                    let host = Host::from_yaml(alias.clone(), host_yaml)?;
                    hosts.insert(alias, host);
                }
            }
            // Some(Yaml::Array(a)) => a,
            Some(_) => {
                bail!("'hosts' entry in config file needs to be dictionary mapping host-alias to configuration!");
            }
            None => {
                debug!("No 'hosts'-entry in config file.");
            }
        };

        let auth = if let Some(Yaml::Hash(auth)) = config_yaml.get(&yaml_string("auth")) {
            match Auth::from_yaml(&auth) {
                Ok(auth) => auth,
                Err(e) => {
                    bail!("Could not read global authentication settings: {}", e);
                }
            }
        } else {
            Auth {
                use_agent: true,
                interactive: true,
            }
        };

        let default_host = get_string_from(config_yaml, "default_host")?.cloned();
        let verify_via_hash = get_bool_from(config_yaml, "verify_via_hash")?
            .cloned()
            .unwrap_or(true);

        Ok(Config {
            auth,
            prefix_length,
            hosts,
            default_host,
            verify_via_hash,
        })
    }

    pub fn get_host<T: AsRef<str>>(&self, alias: Option<T>) -> Result<&Host> {
        match alias
            .as_ref()
            .map(|a| a.as_ref())
            .or(self.default_host.as_deref())
        {
            None => match self.hosts.len() {
                0 => {
                    bail!("No hosts configured, define some!");
                }
                1 => Ok(self.hosts.values().next().unwrap()),
                _ => {
                    bail!("More than one host entry defined but neither `default_host` set in config or --config given via command line.");
                }
            },
            Some(alias) => Ok(self
                .hosts
                .get(alias)
                .with_context(|| format!("Did not find alias: {}", alias))?),
        }
    }
}

impl Host {
    fn from_yaml(alias: String, input: &Yaml) -> Result<Host> {
        if let Yaml::Hash(dict) = input {
            let url = get_required(dict, "url", get_string_from)?.clone();

            let hostname = get_optional(dict, "hostname", get_string_from)?.cloned();

            let user = get_optional(dict, "user", get_string_from)?.cloned();

            let folder = PathBuf::from(get_required(dict, "folder", get_string_from)?);

            let group = get_optional(dict, "group", get_string_from)?.cloned();

            let auth = match get_optional(dict, "auth", get_dict_from)? {
                Some(auth) => Some(Auth::from_yaml(auth)?),
                None => None,
            };

            let prefix_length = match get_optional(dict, "prefix_length", get_int_from)? {
                Some(prefix) => {
                    check_prefix_length(*prefix)?;
                    Some(*prefix as u8)
                }
                None => None,
            };

            let password = get_optional(dict, "password", get_string_from)?.cloned();

            Ok(Host {
                alias,
                auth,
                folder,
                group,
                hostname,
                password,
                prefix_length,
                url,
                user,
            })
        } else {
            bail!("Invalid yaml data for Host-alias '{}'", alias);
        }
    }

    pub fn get_hostname(&self) -> &str {
        self.hostname.as_ref().unwrap_or(&self.alias)
    }

    pub fn get_username(&self) -> String {
        self.user.clone().unwrap_or(username())
    }

    pub fn get_url(&self, file: &str) -> Result<String> {
        Ok(format!(
            "{}/{}",
            &self.url,
            utf8_percent_encode(file, FRAGMENT)
        ))
    }
}

impl Auth {
    fn from_yaml(dict: &Hash) -> Result<Auth, InvalidYamlTypeError> {
        let use_agent = get_bool_from(dict, "use_agent")?.cloned().unwrap_or(true);
        let interactive = get_bool_from(dict, "interactive")?.cloned().unwrap_or(true);

        Ok(Auth {
            use_agent,
            interactive,
        })
    }
}

fn check_prefix_length(length: i64) -> Result<()> {
    if length < 8 || 64 < length {
        bail! {"Prefix needs to be between 8 and 64 characters."};
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::util;

    #[test]
    fn load_example_config() {
        util::test::init().unwrap();
        let cfg = crate::cfg::Config::get("example-config/asfa")
            .unwrap()
            .unwrap();
        log::debug!("Loaded: {:?}", cfg);
        assert_eq!(&cfg.hosts.len(), &2);
        assert_eq!(&cfg.default_host.clone().unwrap(), &"my-remote-site");
        assert_eq!(
            &cfg.get_host(Some("my-remote-site-2")).unwrap().hostname,
            &Some("my-hostname-2.eu".to_string())
        );
    }
}
