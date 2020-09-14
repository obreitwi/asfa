use clap::{crate_authors, crate_description, crate_version, AppSettings, Clap};

use crate::cmd::{Clean, List, Push};

#[derive(Clap, Debug)]
#[clap(
    version=crate_version!(),
    author=crate_authors!(),
    about=crate_description!(),
    global_setting=AppSettings::ColoredHelp,
    global_setting=AppSettings::InferSubcommands)]
pub struct Opts {
    /// Path to configuration folder. Alternatively, ASFA_CONFIG can be set.
    /// [default: ~/.config/asfa]
    #[clap(short = "c", long = "config")]
    pub config: Option<String>,

    /// Set loglevel.
    #[clap(
        short,
        long,
        default_value = "info",
        possible_values = &["trace", "debug", "info", "warn", "error"],
    )]
    pub loglevel: String,

    /// Name of remote site to push to. Only relevant if several remote sites are configured.
    /// The default host can be set in config via `default_host`-option.
    #[clap(short = "H", long)]
    pub host: Option<String>,

    #[clap(subcommand)]
    pub cmd: UserCommand,
}

#[derive(Clap, Debug)]
pub enum UserCommand {
    #[clap(name = "clean")]
    Clean(Clean),

    #[clap(name = "list")]
    List(List),

    #[clap(name = "push")]
    Push(Push),
}
