pub mod config_cmd;
pub mod db;
pub mod doctor;
pub mod down;
pub mod init;
pub mod link;
pub mod logs;
pub mod proxy_cmd;
pub mod ps;
pub mod route;
pub mod ui;
pub mod up;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "conduit",
    version,
    about = "Local dev orchestrator for multi-service Docker Compose projects",
    long_about = "Conduit eliminates port conflicts, provides automatic HTTP routing via named \
                  domains, and offers on-demand database tunnels for Docker Compose projects. \
                  Run `conduit ui` for a local dashboard (projects, routes, and quick commands)."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Enable verbose/debug output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output as JSON (for scripting)
    #[arg(long, global = true)]
    pub json: bool,

    /// Override project directory
    #[arg(long, global = true)]
    pub project_dir: Option<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start a project (parse compose, create network, start containers, configure proxy)
    Up(up::UpArgs),

    /// Stop a project (stop containers, remove network, clean DNS entries)
    Down(down::DownArgs),

    /// List all running projects and their services
    Ps(ps::PsArgs),

    /// Tail logs from one or more services
    Logs(logs::LogsArgs),

    /// Open a temporary TCP tunnel to a database service
    Db(db::DbArgs),

    /// Show the routing table (domains → containers)
    Route(route::RouteArgs),

    /// Connect two project networks
    Link(link::LinkArgs),

    /// Disconnect two project networks
    Unlink(link::UnlinkArgs),

    /// Check system requirements and diagnose issues
    Doctor,

    /// Generate a .conduit.yml from an existing compose file
    Init(init::InitArgs),

    /// Show resolved configuration
    Config(config_cmd::ConfigCmdArgs),

    /// Manage the shared proxy
    Proxy(proxy_cmd::ProxyCmdArgs),

    /// Open a local web dashboard (projects, routes, quick commands)
    Ui(ui::UiArgs),
}

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    let globals = GlobalOpts {
        verbose: cli.verbose,
        json: cli.json,
        project_dir: cli.project_dir,
    };
    match cli.command {
        Command::Up(args) => up::run(args, &globals).await,
        Command::Down(args) => down::run(args, &globals).await,
        Command::Ps(args) => ps::run(args, &globals).await,
        Command::Logs(args) => logs::run(args, &globals).await,
        Command::Db(args) => db::run(args, &globals).await,
        Command::Route(args) => route::run(args, &globals).await,
        Command::Link(args) => link::run_link(args, &globals).await,
        Command::Unlink(args) => link::run_unlink(args, &globals).await,
        Command::Doctor => doctor::run(&globals).await,
        Command::Init(args) => init::run(args, &globals).await,
        Command::Config(args) => config_cmd::run(args, &globals).await,
        Command::Proxy(args) => proxy_cmd::run(args, &globals).await,
        Command::Ui(args) => ui::run(args, &globals).await,
    }
}

/// Global options extracted from Cli, passed by reference to subcommands.
pub struct GlobalOpts {
    /// Enables debug logging (see `main` / `RUST_LOG`).
    #[allow(dead_code)]
    pub verbose: bool,
    pub json: bool,
    pub project_dir: Option<String>,
}
