use clap::Parser;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub(crate) struct Cli {
    #[clap(subcommand)]
    subcommand: SubCommands,

    #[clap(
        short,
        long,
        default_value = "/etc/man10routine/config.toml",
        global = true
    )]
    config: PathBuf,
}

#[derive(Debug, Clone, Subcommand)]
pub(crate) enum SubCommands {
    Daily {},
}
