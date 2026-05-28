use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::docker;
use crate::registry::state;

#[derive(Args)]
pub struct EnvArgs {
    /// Service name
    pub service: String,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,

    /// Get a specific env var value only
    #[arg(short, long)]
    pub key: Option<String>,

    /// Show as JSON
    #[arg(long)]
    pub json: bool,

    /// Show as shell export statements
    #[arg(short, long)]
    pub export: bool,
}

pub async fn run(args: EnvArgs, cli: &GlobalOpts) -> Result<()> {
    let docker = docker::client::connect().await?;
    let conduit_state = state::load()?;

    let (project_name, container_id) = resolve_container(&args, cli, &conduit_state)?;

    let env_map = docker::container::container_env_map(&docker, &container_id).await?;

    if let Some(key) = &args.key {
        match env_map.get(key) {
            Some(val) => println!("{}", val),
            None => anyhow::bail!("Environment variable '{}' not found", key),
        }
        return Ok(());
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&env_map).context("Failed to serialize env")?);
        return Ok(());
    }

    let mut pairs: Vec<_> = env_map.iter().collect();
    pairs.sort_by_key(|(k, _)| (*k).clone());

    if args.export {
        for (k, v) in &pairs {
            println!("export {}={}", k, shell_escape(v));
        }
        return Ok(());
    }

    println!();
    println!(
        "  {} env for {} ({})",
        "Environment".bold(),
        args.service.cyan(),
        project_name
    );
    println!();

    for (k, v) in &pairs {
        let display = if k.to_lowercase().contains("pass")
            || k.to_lowercase().contains("secret")
            || k.to_lowercase().contains("key")
            || k.to_lowercase().contains("token")
        {
            format!("{}={}", k, "*".repeat(v.len().min(20)))
        } else {
            format!("{}={}", k, v)
        };
        println!("  {}", display.cyan());
    }

    println!();
    println!("  {} variables total (use {} for JSON)", pairs.len(), "conduit env --json".cyan());
    println!();

    Ok(())
}

fn shell_escape(s: &str) -> String {
    if s.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '$' || c == '\\') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

fn resolve_container(
    args: &EnvArgs,
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
