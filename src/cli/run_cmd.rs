use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::config;
use crate::registry::state;

#[derive(Args)]
pub struct RunArgs {
    /// Service name
    pub service: String,

    /// Command to run
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,

    /// Environment variables (KEY=VALUE)
    #[arg(short, long)]
    pub env: Vec<String>,

    /// Remove container after exit
    #[arg(long, default_value = "true")]
    pub rm: bool,

    /// No TTY
    #[arg(long)]
    pub no_tty: bool,
}

pub async fn run(args: RunArgs, cli: &GlobalOpts) -> Result<()> {
    let conduit_state = state::load()?;
    let project_name = resolve_project_name(&args.project, cli, &conduit_state)?;
    let project = conduit_state
        .projects
        .get(&project_name)
        .with_context(|| format!("Project '{}' not running", project_name))?;

    let _svc_state = project.services.get(&args.service).with_context(|| {
        let available: Vec<&String> = project.services.keys().collect();
        format!(
            "Service '{}' not found in project '{}'. Available: {}",
            args.service,
            project_name,
            available.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        )
    })?;

    let project_dir = std::path::PathBuf::from(&project.directory);
    let project_config = config::load_project_config(&project_dir).unwrap_or_default();
    let compose_file = project_config
        .compose_file
        .clone()
        .unwrap_or_else(|| "docker-compose.yml".to_string());

    let tty_flag = if args.no_tty { "" } else { "-it" };
    let rm_flag = if args.rm { "--rm" } else { "" };

    let mut cmd_args = vec![
        "compose".to_string(),
        "-f".to_string(),
        compose_file,
    ];

    let compose_project = &project.compose_project_name;
    if !compose_project.is_empty() {
        cmd_args.extend_from_slice(&["-p".to_string(), compose_project.clone()]);
    }

    cmd_args.push("run".to_string());

    let tty = tty_flag.to_string();
    if !tty.is_empty() {
        cmd_args.push(tty);
    }
    let rm = rm_flag.to_string();
    if !rm.is_empty() {
        cmd_args.push(rm);
    }

    for e in &args.env {
        cmd_args.push("-e".to_string());
        cmd_args.push(e.clone());
    }

    cmd_args.push(args.service.clone());
    cmd_args.extend(args.command.clone());

    println!(
        "  {} Running {} {}",
        "→".cyan(),
        args.service.bold(),
        args.command.join(" ").cyan()
    );

    let status = tokio::process::Command::new("docker")
        .args(&cmd_args)
        .current_dir(&project_dir)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to run docker compose run")?;

    if !status.success() {
        anyhow::bail!("Command exited with code {}", status);
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
    anyhow::bail!(
        "No running project found for current directory. Run `conduit up` first or specify --project."
    )
}
