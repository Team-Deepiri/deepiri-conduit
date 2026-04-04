use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use std::path::PathBuf;

use crate::cli::GlobalOpts;
use crate::compose::emit;
use crate::config;
use crate::dns;
use crate::docker;
use crate::project_id;
use crate::proxy;
use crate::registry::state;

#[derive(Args)]
pub struct DownArgs {
    /// Also remove named volumes (data loss!)
    #[arg(short, long)]
    pub volumes: bool,

    /// Stop ALL conduit-managed projects
    #[arg(long)]
    pub all: bool,

    /// Graceful shutdown timeout in seconds
    #[arg(long, default_value = "10")]
    pub timeout: u64,
}

pub async fn run(args: DownArgs, cli: &GlobalOpts) -> Result<()> {
    let docker = docker::client::connect().await?;

    if args.all {
        return stop_all(&docker, &args).await;
    }

    let project_dir = match &cli.project_dir {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir().context("Failed to get current directory")?,
    };

    let project_config = config::load_project_config(&project_dir)?;
    let project_name = project_config
        .project
        .clone()
        .or_else(|| {
            project_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "default".to_string());

    stop_project(&docker, &project_name, &project_dir, &args).await
}

async fn stop_project(
    docker: &bollard::Docker,
    project_name: &str,
    project_dir: &PathBuf,
    args: &DownArgs,
) -> Result<()> {
    println!("  {} Stopping project {}", "→".cyan(), project_name.bold());

    let conduit_state = state::load()?;
    let project_state = conduit_state.projects.get(project_name).cloned();

    let (generated_rel, compose_project_name) = if let Some(ref ps) = project_state {
        let gen = if ps.generated_compose.is_empty() {
            emit::GENERATED_REL_PATH.to_string()
        } else {
            ps.generated_compose.clone()
        };
        let cp = if ps.compose_project_name.is_empty() {
            project_id::sanitize_compose_project(project_name)
        } else {
            ps.compose_project_name.clone()
        };
        (gen, cp)
    } else {
        (
            emit::GENERATED_REL_PATH.to_string(),
            project_id::sanitize_compose_project(project_name),
        )
    };

    let compose_file_for_down = project_dir.join(&generated_rel);
    if !compose_file_for_down.exists() {
        eprintln!(
            "  {} Missing {} — falling back to docker compose down without -p",
            "⚠".yellow(),
            compose_file_for_down.display()
        );
    }

    let mut cmd_args = vec![
        "compose".to_string(),
        "-f".to_string(),
        generated_rel.clone(),
        "-p".to_string(),
        compose_project_name.clone(),
        "down".to_string(),
        "--remove-orphans".to_string(),
    ];

    if args.volumes {
        cmd_args.push("--volumes".to_string());
    }

    let output = tokio::process::Command::new("docker")
        .args(&cmd_args)
        .current_dir(project_dir)
        .output()
        .await
        .context("Failed to run docker compose down")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("  {} docker compose down: {}", "⚠".yellow(), stderr.trim());
    }

    let stopped = docker::container::stop_project_containers(docker, project_name).await?;
    let removed = docker::container::remove_project_containers(docker, project_name).await?;

    if let Some(ps) = project_state {
        if !ps.network.is_empty() {
            proxy::manager::disconnect_from_project_network(docker, &ps.network)
                .await
                .ok();
            docker::network::remove_network(docker, &ps.network)
                .await
                .ok();
        }
    }

    state::remove_project(project_name)?;

    if let Ok(s) = state::load() {
        dns::hosts::sync_from_state(&s).ok();
    }

    println!(
        "  {} Project {} stopped ({} containers removed)",
        "✓".green(),
        project_name.bold(),
        removed.max(stopped)
    );

    if args.volumes {
        println!("  {} Volumes removed", "✓".green());
    }

    Ok(())
}

async fn stop_all(docker: &bollard::Docker, args: &DownArgs) -> Result<()> {
    let projects = state::list_projects()?;
    if projects.is_empty() {
        println!("  {} No conduit-managed projects running", "ℹ".blue());
        return Ok(());
    }

    println!(
        "  {} Stopping {} project{}",
        "→".cyan(),
        projects.len(),
        if projects.len() == 1 { "" } else { "s" }
    );

    for project_name in &projects {
        let project_state = state::get_project(project_name)?;
        let project_dir = project_state
            .as_ref()
            .map(|ps| PathBuf::from(&ps.directory))
            .unwrap_or_else(|| PathBuf::from("."));

        stop_project(docker, project_name, &project_dir, args).await?;
    }

    let remaining = docker::container::list_all_conduit_containers(docker).await?;
    if remaining.is_empty() {
        proxy::manager::stop(docker).await.ok();
        println!("  {} Proxy stopped (no projects remaining)", "✓".green());
    }

    if let Ok(s) = state::load() {
        dns::hosts::sync_from_state(&s).ok();
    }

    Ok(())
}
