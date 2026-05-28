use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::docker;
use crate::registry::state;

#[derive(Args)]
pub struct DescribeArgs {
    /// Service name
    pub service: String,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,

    /// Show raw Docker inspect JSON
    #[arg(long)]
    pub raw: bool,
}

pub async fn run(args: DescribeArgs, cli: &GlobalOpts) -> Result<()> {
    let docker = docker::client::connect().await?;
    let conduit_state = state::load()?;

    let (project_name, container_id) = resolve_container(&args, cli, &conduit_state)?;

    let info = docker.inspect_container(&container_id, None).await?;

    if args.raw {
        println!(
            "{}",
            serde_json::to_string_pretty(&info).context("Failed to serialize inspect")?
        );
        return Ok(());
    }

    let cfg = info.config.as_ref();
    let state_info = info.state.as_ref();
    let network_settings = info.network_settings.as_ref();
    let host_config = info.host_config.as_ref();
    let name_val = info.name.as_deref().unwrap_or("?").trim_start_matches('/');
    let image = cfg.and_then(|c| c.image.as_deref()).unwrap_or("?");
    let created = info.created.as_deref().unwrap_or("?");

    println!();
    println!("  {} {} ({})", "Service:".bold(), args.service.bold(), project_name);
    println!("  {}     {}", "Container:".bold(), name_val);
    println!("  {}     {}", "Image:".bold(), image);
    println!("  {}     {}", "Created:".bold(), created);
    println!();

    if let Some(s) = state_info {
        println!("  {} {:?}", "Status:".bold(), s.status);
        println!("  {} {}", "Running:".bold(), s.running.unwrap_or(false));
        println!("  {} {}", "Exit Code:".bold(), s.exit_code.unwrap_or(-1));
        if let Some(h) = &s.health {
            println!("  {} {:?}", "Health:".bold(), h.status);
        }
    }
    println!();

    if let Some(c) = cfg {
        println!("  {} {}", "Entrypoint:".bold(), c.entrypoint.as_ref().map(|e| format!("{:?}", e)).unwrap_or_else(|| "—".to_string()));
        println!("  {} {}", "Command:".bold(), c.cmd.as_ref().map(|e| format!("{:?}", e)).unwrap_or_else(|| "—".to_string()));
        println!("  {} {}", "User:".bold(), c.user.as_deref().unwrap_or("—"));
        println!("  {} {}", "Working Dir:".bold(), c.working_dir.as_deref().unwrap_or("—"));
    }
    println!();

    if let Some(ns) = network_settings {
        println!("  {} {}", "Networks:".bold(), ns.networks.as_ref().map(|n| {
            n.keys().cloned().collect::<Vec<_>>().join(", ")
        }).unwrap_or_else(|| "—".to_string()));

        if let Some(ip) = ns.ip_address.as_ref().filter(|s| !s.is_empty()) {
            println!("  {}     {}", "IP Address:".bold(), ip);
        }
    }
    println!();

    if let Some(hc) = host_config {
        if let Some(memory) = hc.memory {
            println!("  {} {} MB", "Memory Limit:".bold(), memory / (1024 * 1024));
        }
        if let Some(nano_cpus) = hc.nano_cpus {
            println!("  {} {} CPUs", "CPU Limit:".bold(), nano_cpus as f64 / 1_000_000_000.0);
        }
        if let Some(restart) = &hc.restart_policy {
            if let Some(name) = &restart.name {
                println!("  {} {:?}", "Restart Policy:".bold(), name);
            }
        }
    }
    println!();

    if let Some(c) = cfg {
        if let Some(env) = &c.env {
            println!("  {} (first 15 shown)", "Environment:".bold());
            for line in env.iter().take(15) {
                if let Some((k, v)) = line.split_once('=') {
                    let masked = if k.to_lowercase().contains("pass") || k.to_lowercase().contains("secret") || k.to_lowercase().contains("key") || k.to_lowercase().contains("token") {
                        format!("{}={}", k, "*".repeat(v.len().min(20)))
                    } else {
                        line.clone()
                    };
                    println!("    {}", masked.cyan());
                } else {
                    println!("    {}", line.cyan());
                }
            }
            if env.len() > 15 {
                println!("    ... {} more (use {} for full env)", env.len() - 15, "conduit env".cyan());
            }
        }
    }

    if let Some(c) = cfg {
        if let Some(labels) = &c.labels {
            let conduit_labels: Vec<_> = labels.iter().filter(|(k, _)| k.starts_with("conduit.") || k.starts_with("traefik.")).collect();
            if !conduit_labels.is_empty() {
                println!();
                println!("  {} ({})", "Conduit Labels:".bold(), conduit_labels.len());
                for (k, v) in conduit_labels {
                    println!("    {}={}", k.cyan(), v);
                }
            }
        }
    }

    println!();
    Ok(())
}

fn resolve_container(
    args: &DescribeArgs,
    cli: &GlobalOpts,
    state: &state::ConduitState,
) -> Result<(String, String)> {
    let project_name = if let Some(name) = &args.project {
        name.clone()
    } else {
        let current_dir = cli.project_dir.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        });
        let mut found = None;
        for (name, project) in &state.projects {
            if project.directory == current_dir || project.directory.ends_with(&current_dir) {
                found = Some(name.clone());
                break;
            }
        }
        found.with_context(|| {
            "No running project found for current directory. Use --project or run `conduit up` first."
        })?
    };

    let project = state
        .projects
        .get(&project_name)
        .with_context(|| format!("Project '{}' not found in state", project_name))?;

    let svc = project.services.get(&args.service).with_context(|| {
        let available: Vec<&String> = project.services.keys().collect();
        format!(
            "Service '{}' not found in project '{}'. Available: {}",
            args.service,
            project_name,
            available.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        )
    })?;

    Ok((project_name, svc.container_id.clone()))
}
