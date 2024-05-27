#![cfg_attr(feature = "doc", doc = include_str!("../README.md"))]
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

use clap::Parser;

fn main() {
    if let Err(err) = try_main() {
        log::error!("{}", err);
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
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
        (None, 0, 2) => log::LevelFilter::Error,
        (None, 0, _) => log::LevelFilter::Off,
        _ => {
            bail!("Restriction of loglevel in clap failed!");
        }
    };

    SimpleLogger::new().with_level(level).init()?;

    trace!("Opts: {:?}", opts);

    let env_cfg_path = std::env::var("ASFA_CONFIG").ok();

    let cfg = {
        let mut cfg = cfg::load(&opts.config.or(env_cfg_path))?;
        cfg.loglevel = level;
        cfg
    };
    let host = cfg.get_host(opts.host)?;

    trace!("Config file: {:#?}", cfg);
    trace!("Host: {:?}", host);

    let session = SshSession::connect(&host)?;

    use cli::UserCommand::*;
    match opts.cmd {
        // there is no dispatch over all enum variants? Boo!
        Check(cmd) => cmd.run(&session, &cfg),
        Clean(cmd) => cmd.run(&session, &cfg),
        List(cmd) => cmd.run(&session, &cfg),
        Mv(cmd) => cmd.run(&session, &cfg),
        Push(cmd) => cmd.run(&session, &cfg),
        Rename(cmd) => cmd.run(&session, &cfg),
        Verify(cmd) => cmd.run(&session, &cfg),
    }?;
    Ok(())
}
