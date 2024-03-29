use crate::cfg::Config;
use crate::ssh::SshSession;

use anyhow::Result;

mod check;
mod clean;
mod list;
mod push;
mod rename;
mod verify;

pub use check::Check;
pub use clean::Clean;
pub use list::List;
pub use push::Push;
pub use rename::Rename;
pub use verify::Verify;

pub trait Command {
    /// Run the given command
    fn run(&self, session: &SshSession, config: &Config) -> Result<()>;
}
