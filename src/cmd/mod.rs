use crate::cfg::Config;
use crate::ssh::SshSession;

use anyhow::Result;

mod clean;
mod list;
mod push;

pub use clean::Clean;
pub use list::List;
pub use push::Push;

pub trait Command {
    /// Run the given command
    fn run(&self, session: &SshSession, config: &Config) -> Result<()>;
}
