use anyhow::Result;
use clap::Args;

use crate::cli::GlobalOpts;
use crate::ui::server;

#[derive(Args)]
pub struct UiArgs {
    /// Port for the local dashboard (default: 9842)
    #[arg(short, long, default_value_t = 9842)]
    pub port: u16,

    /// Do not open a browser window
    #[arg(long)]
    pub no_open: bool,
}

pub async fn run(args: UiArgs, _cli: &GlobalOpts) -> Result<()> {
    server::run_dashboard(args.port, !args.no_open).await
}
