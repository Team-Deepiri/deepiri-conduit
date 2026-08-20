//! deepiri-conduit — local Docker Compose dev orchestration (library + `conduit` binary).

pub mod cli;
pub mod compose;
pub mod config;
pub mod dns;
pub mod docker;
pub mod project_id;
pub mod proxy;
pub mod registry;
pub mod submod;
pub mod tunnel;
pub mod ui;

/// Entry point used by the `conduit` binary after CLI parse and tracing setup.
pub async fn run(cli: cli::Cli) -> anyhow::Result<()> {
    cli::run(cli).await
}
