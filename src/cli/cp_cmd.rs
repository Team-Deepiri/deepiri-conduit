use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::registry::state;

#[derive(Args)]
pub struct CpArgs {
    /// Source path (e.g., "service:/path" or "/local/path")
    pub source: String,

    /// Destination path (e.g., "/local/path" or "service:/path")
    pub destination: String,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,
}

pub async fn run(args: CpArgs, cli: &GlobalOpts) -> Result<()> {
    let conduit_state = state::load()?;
    let project_name = resolve_project_name(&args.project, cli, &conduit_state)?;
    let project = conduit_state
        .projects
        .get(&project_name)
        .with_context(|| format!("Project '{}' not running", project_name))?;

    let resolve_path = |path: &str| -> Result<String> {
        if let Some((svc_name, rest)) = path.split_once(':') {
            let svc = project.services.get(svc_name).with_context(|| {
                format!(
                    "Service '{}' not found in project '{}'",
                    svc_name, project_name
                )
            })?;
            Ok(format!("{}:{}", svc.container_name, rest))
        } else {
            let path = std::path::Path::new(path);
            let abs = if path.is_relative() {
                std::env::current_dir()?.join(path)
            } else {
                path.to_path_buf()
            };
            Ok(abs.to_string_lossy().to_string())
        }
    };

    let resolved_source = resolve_path(&args.source)?;
    let resolved_dest = resolve_path(&args.destination)?;

    println!(
        "  {} {} → {}",
        "→".cyan(),
        resolved_source.cyan(),
        resolved_dest.cyan()
    );

    let status = tokio::process::Command::new("docker")
        .args(["cp", &resolved_source, &resolved_dest])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to run docker cp")?;

    if !status.success() {
        anyhow::bail!("docker cp failed with code {}", status);
    }

    Ok(())
}

fn resolve_project_name(
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
    if let Some(first) = state.projects.keys().next() {
        return Ok(first.clone());
    }
    anyhow::bail!("No running projects found. Run `conduit up` first.");
}
