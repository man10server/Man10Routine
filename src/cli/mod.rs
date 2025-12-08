use clap::Parser;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(
    name = env!("CARGO_PKG_NAME"),
    version = env!("CARGO_PKG_VERSION"),
)]
pub(crate) struct Cli {
    #[clap(subcommand)]
    pub(crate) routine: Routine,

    #[clap(
        long = "config",
        default_value = "/etc/man10routine/config.toml",
        global = true
    )]
    pub(crate) config: PathBuf,
}

#[derive(Debug, Clone, Subcommand)]
pub(crate) enum Routine {
    Daily {},
}
