use anyhow::Result;
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::registry::state;

#[derive(Args)]
pub struct RouteArgs {
    /// JSON output
    #[arg(long)]
    pub json: bool,

    /// Filter by project
    #[arg(long)]
    pub project: Option<String>,
}

pub async fn run(args: RouteArgs, cli: &GlobalOpts) -> Result<()> {
    let conduit_state = state::load()?;

    if cli.json || args.json {
        let json = serde_json::to_string_pretty(&conduit_state.projects)?;
        println!("{}", json);
        return Ok(());
    }

    let mut has_routes = false;

    println!();
    println!(
        "  {:<40} {:<40} {}",
        "DOMAIN".bold(),
        "TARGET".bold(),
        "TLS".bold(),
    );
    println!("  {}", "─".repeat(85));

    for (name, project) in &conduit_state.projects {
        if let Some(filter) = &args.project {
            if name != filter {
                continue;
            }
        }

        for (domain, target) in &project.routes {
            has_routes = true;
            println!("  {:<40} {:<40} {}", domain.cyan(), target, "✓".green(),);
        }
    }

    if !conduit_state.tunnels.is_empty() {
        println!();
        println!("  {} ACTIVE TUNNELS:", "".bold());
        for (key, tunnel) in &conduit_state.tunnels {
            println!(
                "  localhost:{:<25} {}:{}{}",
                tunnel.host_port.to_string().cyan(),
                tunnel.container_name,
                tunnel.container_port,
                format!("  ({})", key).dimmed(),
            );
        }
    }

    if !has_routes && conduit_state.tunnels.is_empty() {
        println!(
            "  {} No routes configured. Run {} first.",
            "ℹ".blue(),
            "conduit up".cyan()
        );
    }

    println!();
    Ok(())
}
