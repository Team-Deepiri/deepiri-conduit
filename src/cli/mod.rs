pub mod bench;
pub mod config_cmd;
pub mod config_validate;
pub mod connect;
pub mod cp_cmd;
pub mod db;
pub mod describe;
pub mod doctor;
pub mod down;
pub mod env_cmd;
pub mod exec_cmd;
pub mod graph;
pub mod image;
pub mod init;
pub mod link;
pub mod logs;
pub mod port_forward;
pub mod proxy_cmd;
pub mod ps;
pub mod route;
pub mod run_cmd;
pub mod snapshot;
pub mod submod;
pub mod top;
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

    /// Resolve submodule conflicts between two branches
    Submod(submod::SubmodArgs),

    /// Exec into a running container by service name
    Exec(exec_cmd::ExecArgs),

    /// Copy files between host and containers by service name
    Cp(cp_cmd::CpArgs),

    /// Run a one-off command in a service context
    Run(run_cmd::RunArgs),

    /// Forward a TCP port from a service container
    PortForward(port_forward::PortForwardArgs),

    /// Show detailed information about a service container
    Describe(describe::DescribeArgs),

    /// Show environment variables for a service
    #[command(name = "env")]
    Env(env_cmd::EnvArgs),

    /// Show dependency graph for a project
    Graph(graph::GraphArgs),

    /// HTTP health check benchmark against project routes
    Bench(bench::BenchArgs),

    /// Validate project configuration
    #[command(name = "config")]
    ConfigValidate(config_validate::ConfigValidateArgs),

    /// Manage Docker images
    Image(image::ImageArgs),

    /// Show resource usage for running containers
    Top(top::TopArgs),

    /// Manage data volume snapshots
    Snapshot(snapshot::SnapshotArgs),

    /// Connect to remote Docker hosts via SSH tunnel
    Connect(connect::ConnectArgs),
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
        Command::Submod(args) => submod::run(args, &globals).await,
        Command::Exec(args) => exec_cmd::run(args, &globals).await,
        Command::Cp(args) => cp_cmd::run(args, &globals).await,
        Command::Run(args) => run_cmd::run(args, &globals).await,
        Command::PortForward(args) => port_forward::run(args, &globals).await,
        Command::Describe(args) => describe::run(args, &globals).await,
        Command::Env(args) => env_cmd::run(args, &globals).await,
        Command::Graph(args) => graph::run(args, &globals).await,
        Command::Bench(args) => bench::run(args, &globals).await,
        Command::ConfigValidate(args) => config_validate::run(args, &globals).await,
        Command::Image(args) => image::run(args, &globals).await,
        Command::Top(args) => top::run(args, &globals).await,
        Command::Snapshot(args) => snapshot::run(args, &globals).await,
        Command::Connect(args) => connect::run(args, &globals).await,
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
