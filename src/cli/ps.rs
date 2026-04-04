use anyhow::Result;
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::docker;
use crate::proxy;
use crate::registry::state;

#[derive(Args)]
pub struct PsArgs {
    /// Show stopped services too
    #[arg(short, long)]
    pub all: bool,

    /// JSON output
    #[arg(long)]
    pub json: bool,

    /// Detailed view with ports, health, domains
    #[arg(short, long)]
    pub wide: bool,
}

pub async fn run(args: PsArgs, cli: &GlobalOpts) -> Result<()> {
    let conduit_state = state::load()?;

    if conduit_state.projects.is_empty() {
        if cli.json || args.json {
            println!("{{\"projects\":[]}}");
        } else {
            println!("  {} No conduit-managed projects running", "ℹ".blue());
            println!("  Run {} to start a project", "conduit up".cyan());
        }
        return Ok(());
    }

    if cli.json || args.json {
        let json = serde_json::to_string_pretty(&conduit_state)?;
        println!("{}", json);
        return Ok(());
    }

    if args.wide {
        print_wide(&conduit_state).await?;
    } else {
        print_summary(&conduit_state).await?;
    }

    Ok(())
}

async fn print_summary(conduit_state: &state::ConduitState) -> Result<()> {
    println!(
        "\n  {:<20} {:<12} {:<10} {:<30} {}",
        "PROJECT".bold(),
        "SERVICES".bold(),
        "STATUS".bold(),
        "NETWORK".bold(),
        "UPTIME".bold(),
    );
    println!("  {}", "─".repeat(85));

    let docker = docker::client::connect().await.ok();

    for (name, project) in &conduit_state.projects {
        let total = project.services.len();
        let running = project
            .services
            .values()
            .filter(|s| s.status == "running")
            .count();

        let status = if running == total {
            "healthy".green()
        } else if running > 0 {
            "partial".yellow()
        } else {
            "stopped".red()
        };

        let uptime = format_duration(Utc::now() - project.started_at);

        println!(
            "  {:<20} {:<12} {:<10} {:<30} {}",
            name,
            format!("{}/{}", running, total),
            status,
            &project.network,
            uptime,
        );
    }

    if let Some(docker) = &docker {
        let proxy_status = proxy::manager::get_proxy_status(docker).await?;
        println!();
        match proxy_status {
            Some(status) if status == "running" => {
                println!("  PROXY: {} (traefik) on :80/:443", "running".green());
            }
            Some(status) => {
                println!("  PROXY: {}", status.yellow());
            }
            None => {
                println!("  PROXY: {}", "not running".red());
            }
        }
    }

    let tunnel_count = conduit_state.tunnels.len();
    if tunnel_count > 0 {
        println!("  TUNNELS: {} active", tunnel_count);
    }

    println!();
    Ok(())
}

async fn print_wide(conduit_state: &state::ConduitState) -> Result<()> {
    let docker = docker::client::connect().await.ok();

    for (name, project) in &conduit_state.projects {
        let uptime = format_duration(Utc::now() - project.started_at);
        println!();
        println!("  PROJECT: {} ({})", name.bold(), project.compose_file);
        println!("  NETWORK: {}", project.network);
        println!("  UPTIME:  {}", uptime);
        println!();
        println!(
            "  {:<40} {:<10} {:<12} {}",
            "SERVICE".bold(),
            "STATUS".bold(),
            "HEALTH".bold(),
            "DOMAIN".bold(),
        );
        println!("  {}", "─".repeat(80));

        let mut services: Vec<_> = project.services.iter().collect();
        services.sort_by_key(|(name, _)| (*name).clone());

        for (svc_name, svc) in services {
            let status_colored = match svc.status.as_str() {
                "running" => "running".green(),
                "exited" => "exited".red(),
                _ => svc.status.as_str().yellow(),
            };

            let health = if let Some(docker) = &docker {
                docker::container::get_health_status(docker, &svc.container_id)
                    .await
                    .map(|h| {
                        let s = h.to_string();
                        match h {
                            docker::container::HealthStatus::Healthy => s.green(),
                            docker::container::HealthStatus::Unhealthy => s.red(),
                            docker::container::HealthStatus::Starting => s.yellow(),
                            _ => s.normal(),
                        }
                    })
                    .unwrap_or_else(|_| "—".normal())
            } else {
                "—".normal()
            };

            let domain = svc.domain.as_deref().unwrap_or("—");

            println!(
                "  {:<40} {:<10} {:<12} {}",
                svc_name, status_colored, health, domain
            );
        }
    }

    println!();
    Ok(())
}

use chrono::{Duration, Utc};

fn format_duration(d: Duration) -> String {
    let secs = d.num_seconds();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{}h {}m", h, m)
    } else {
        let d = secs / 86400;
        let h = (secs % 86400) / 3600;
        format!("{}d {}h", d, h)
    }
}
