use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::config;
use crate::docker;
use crate::registry::state;
use crate::tunnel::tcp::TcpTunnel;

#[derive(Args)]
pub struct PortForwardArgs {
    /// Service name
    pub service: String,

    /// Container port to forward
    pub port: u16,

    /// Local port (optional, auto-assigned if not specified)
    #[arg(short, long)]
    pub local_port: Option<u16>,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,

    /// Protocol (tcp or udp)
    #[arg(long, default_value = "tcp")]
    pub protocol: String,
}

pub async fn run(args: PortForwardArgs, cli: &GlobalOpts) -> Result<()> {
    let docker = docker::client::connect().await?;
    let global_config = config::load_global_config()?;
    let conduit_state = state::load()?;

    let project_name = resolve_project(&args.project, cli, &conduit_state)?;
    let project = conduit_state
        .projects
        .get(&project_name)
        .with_context(|| format!("Project '{}' not running", project_name))?;

    let svc_state = project.services.get(&args.service).with_context(|| {
        let available: Vec<&String> = project.services.keys().collect();
        format!(
            "Service '{}' not found in project '{}'. Available: {}",
            args.service,
            project_name,
            available.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        )
    })?;

    let container_ip =
        docker::network::get_container_ip(&docker, &svc_state.container_id, &project.network)
            .await
            .with_context(|| {
                format!(
                    "Cannot find IP for container {}. Is it running?",
                    svc_state.container_name
                )
            })?;

    let tunnel = TcpTunnel::start(
        &container_ip,
        args.port,
        args.local_port,
        global_config.tunnels.default_range,
    )
    .await?;

    println!();
    println!(
        "  {} Forward: localhost:{} → {}:{} ({})",
        "✓".green(),
        tunnel.host_port.to_string().cyan(),
        svc_state.container_name,
        args.port,
        args.protocol
    );
    println!(
        "  {} Active connections: {}",
        "ℹ".blue(),
        tunnel.active_connections.load(std::sync::atomic::Ordering::Relaxed)
    );
    println!();
    println!("  Forward active. Press Ctrl+C to close.");
    println!();

    tokio::signal::ctrl_c().await?;

    println!(
        "  {} Forward closed ({} total connections)",
        "✓".green(),
        tunnel
            .total_connections
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    tunnel.stop();

    Ok(())
}

fn resolve_project(
    project_arg: &Option<String>,
    cli: &GlobalOpts,
    state: &state::ConduitState,
) -> Result<String> {
    if let Some(name) = project_arg {
        return Ok(name.clone());
    }
    let current_dir = cli.project_dir.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    for (name, project) in &state.projects {
        if project.directory == current_dir || project.directory.ends_with(&current_dir) {
            return Ok(name.clone());
        }
    }
    anyhow::bail!(
        "No running project found for current directory. Run `conduit up` first or specify --project."
    )
}
