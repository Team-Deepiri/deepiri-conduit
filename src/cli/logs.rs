use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use std::path::PathBuf;

use crate::cli::GlobalOpts;
use crate::compose::emit;
use crate::config;
use crate::project_id;
use crate::registry::state;

#[derive(Args)]
pub struct LogsArgs {
    /// Service names to tail (omit for all)
    pub services: Vec<String>,

    /// Follow log output
    #[arg(short, long, default_value = "true")]
    pub follow: bool,

    /// Number of lines from end
    #[arg(long, default_value = "50")]
    pub tail: String,

    /// Show logs since duration (e.g., 5m, 1h)
    #[arg(long)]
    pub since: Option<String>,

    /// Show logs for a service group
    #[arg(long)]
    pub group: Option<String>,
}

pub async fn run(args: LogsArgs, cli: &GlobalOpts) -> Result<()> {
    let project_dir = match &cli.project_dir {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir()?,
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

    let compose_project = state::get_project(&project_name)?
        .filter(|ps| !ps.compose_project_name.is_empty())
        .map(|ps| ps.compose_project_name)
        .unwrap_or_else(|| project_id::sanitize_compose_project(&project_name));

    let generated = project_dir.join(emit::GENERATED_REL_PATH);
    let compose_path = if generated.exists() {
        generated
    } else if let Some(p) = project_config.compose_file.as_ref() {
        let p = project_dir.join(p);
        if p.exists() {
            p
        } else {
            crate::compose::parser::find_compose_file(&project_dir)
                .context("No compose file found")?
        }
    } else {
        crate::compose::parser::find_compose_file(&project_dir).context("No compose file found")?
    };

    run_docker_logs(&project_dir, &compose_path, &compose_project, &args).await
}

async fn run_docker_logs(
    project_dir: &std::path::Path,
    compose_path: &std::path::Path,
    compose_project: &str,
    args: &LogsArgs,
) -> Result<()> {
    let filename = compose_path
        .strip_prefix(project_dir)
        .unwrap_or(compose_path)
        .to_string_lossy()
        .to_string();

    let mut cmd_args = vec![
        "compose".to_string(),
        "-f".to_string(),
        filename,
        "-p".to_string(),
        compose_project.to_string(),
        "logs".to_string(),
        "--tail".to_string(),
        args.tail.clone(),
    ];

    if args.follow {
        cmd_args.push("--follow".to_string());
    }

    if let Some(since) = &args.since {
        cmd_args.push("--since".to_string());
        cmd_args.push(since.clone());
    }

    for svc in &args.services {
        cmd_args.push(svc.clone());
    }

    let status = tokio::process::Command::new("docker")
        .args(&cmd_args)
        .current_dir(project_dir)
        .status()
        .await
        .context("Failed to run docker compose logs")?;

    if !status.success() {
        eprintln!("  {} Logs command exited with error", "⚠".yellow());
    }

    Ok(())
}
