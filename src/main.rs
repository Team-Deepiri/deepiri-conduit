//! `conduit` binary — thin wrapper around [`deepiri_conduit::run`].

use clap::Parser;
use deepiri_conduit::cli::Cli;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            if cli.verbose {
                EnvFilter::new("debug")
            } else {
                EnvFilter::new("warn")
            }
        }))
        .without_time()
        .init();

    deepiri_conduit::run(cli).await
}
