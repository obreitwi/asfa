#![cfg_attr(feature = "doc", feature(external_doc))]
#![cfg_attr(feature = "doc", doc(include = "../README.md"))]
#![forbid(unsafe_code)]

mod cfg;
mod cli;
mod cmd;
mod ssh;
mod util;

use anyhow::{bail, Result};
use cmd::Command;
use log::debug;
use ssh::SshSession;

use simple_logger::SimpleLogger;

use clap::Clap;

fn main() -> Result<()> {
    let opts = cli::Opts::parse();

    let level = match opts.loglevel.as_str() {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ => {
            bail!("Restriction of loglevel in clap failed!");
        }
    };

    SimpleLogger::new().with_level(level).init()?;

    debug!("Opts: {:?}", opts);

    let env_cfg_path = std::env::var("ASFA_CONFIG").ok();

    let cfg = cfg::get(&opts.config.or(env_cfg_path))?;
    let host = cfg.get_host(opts.host)?;

    debug!("Config file: {:?}", cfg);
    debug!("Host: {:?}", host);

    let session = SshSession::create(&host, &cfg.auth)?;

    use cli::UserCommand::*;
    match opts.cmd {
        // there is no dispatch over all enum variants? Boo!
        Clean(cmd) => cmd.run(&session, &cfg),
        List(cmd) => cmd.run(&session, &cfg),
        Push(cmd) => cmd.run(&session, &cfg),
    }?;
    Ok(())
}
