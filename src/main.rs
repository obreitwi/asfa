#![cfg_attr(feature = "doc", feature(external_doc))]
#![cfg_attr(feature = "doc", doc(include = "../README.md"))]
#![forbid(unsafe_code)]

mod at;
mod cfg;
mod cli;
mod cmd;
mod file_listing;
mod openssh;
mod ssh;
mod util;

use anyhow::{bail, Result};
use cmd::Command;
use log::trace;
use ssh::SshSession;

use simple_logger::SimpleLogger;

use clap::Clap;

fn main() -> Result<()> {
    let opts = cli::Opts::parse();

    opts.verify()?;

    let level = match (opts.loglevel.as_deref(), opts.verbose, opts.quiet) {
        (Some("trace"), _, _) => log::LevelFilter::Trace,
        (Some("debug"), _, _) => log::LevelFilter::Debug,
        (Some("info"), _, _) => log::LevelFilter::Info,
        (Some("warn"), _, _) => log::LevelFilter::Warn,
        (Some("error"), _, _) => log::LevelFilter::Error,
        (None, v, 0) if v > 1 => log::LevelFilter::Trace,
        (None, 1, 0) => log::LevelFilter::Debug,
        (None, 0, 0) => log::LevelFilter::Info,
        (None, 0, 1) => log::LevelFilter::Warn,
        (None, 0, q) if q > 1 => log::LevelFilter::Error,
        _ => {
            bail!("Restriction of loglevel in clap failed!");
        }
    };

    SimpleLogger::new().with_level(level).init()?;

    trace!("Opts: {:?}", opts);

    let env_cfg_path = std::env::var("ASFA_CONFIG").ok();

    let cfg = cfg::load(&opts.config.or(env_cfg_path))?;
    let host = cfg.get_host(opts.host)?;

    trace!("Config file: {:#?}", cfg);
    trace!("Host: {:?}", host);

    let session = SshSession::create(&host)?;

    use cli::UserCommand::*;
    match opts.cmd {
        // there is no dispatch over all enum variants? Boo!
        Clean(cmd) => cmd.run(&session, &cfg),
        List(cmd) => cmd.run(&session, &cfg),
        Push(cmd) => cmd.run(&session, &cfg),
        Verify(cmd) => cmd.run(&session, &cfg),
    }?;
    Ok(())
}
