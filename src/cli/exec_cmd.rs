use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::docker;
use crate::registry::state;

#[derive(Args)]
pub struct ExecArgs {
    /// Service name (e.g., api-gateway, postgres)
    pub service: String,

    /// Command to run (default: /bin/sh)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, default_value = "/bin/sh")]
    pub command: Vec<String>,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,

    /// Run without TTY allocation
    #[arg(long)]
    pub no_tty: bool,

    /// Run as non-interactive (capture output)
    #[arg(long)]
    pub capture: bool,
}

pub async fn run(args: ExecArgs, cli: &GlobalOpts) -> Result<()> {
    let docker = docker::client::connect().await?;
    let containers = docker::container::list_all_conduit_containers(&docker).await?;

    let (_project_name, container_name, service) = resolve_target(&args, cli, &containers)?;

    if args.capture {
        let output = tokio::process::Command::new("docker")
            .args([
                "exec",
                &container_name,
            ])
            .args(&args.command)
            .output()
            .await
            .with_context(|| format!("Failed to exec into {}", container_name))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.is_empty() {
            print!("{stdout}");
        }
        if !stderr.is_empty() {
            eprint!("{stderr}");
        }

        return Ok(());
    }

    let tty_flag = if args.no_tty { "--no-TTY" } else { "-it" };

    println!(
        "  {} Exec {} → {} {}",
        "→".cyan(),
        service.bold(),
        container_name.bold(),
        args.command.join(" ").cyan()
    );

    let mut cmd = tokio::process::Command::new("docker");
    cmd.args(["exec", tty_flag, &container_name])
        .args(&args.command)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    let status = cmd.status().await.with_context(|| {
        format!("Failed to exec into container {}", container_name)
    })?;

    if !status.success() {
        anyhow::bail!("Command exited with code {}", status);
    }

    Ok(())
}

fn resolve_target(
    args: &ExecArgs,
    cli: &GlobalOpts,
    containers: &[docker::container::ContainerInfo],
) -> Result<(String, String, String)> {
    let conduit_state = state::load()?;

    if let Some(ref project_name) = args.project {
        let project = conduit_state
            .projects
            .get(project_name)
            .with_context(|| format!("Project '{}' not found in state", project_name))?;

        let svc = project
            .services
            .get(&args.service)
            .with_context(|| {
                let available: Vec<&String> = project.services.keys().collect();
                format!(
                    "Service '{}' not found in project '{}'. Available: {}",
                    args.service,
                    project_name,
                    available.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                )
            })?;

        return Ok((
            project_name.clone(),
            svc.container_name.clone(),
            args.service.clone(),
        ));
    }

    let current_dir = cli.project_dir.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    });

    for (name, project) in &conduit_state.projects {
        if project.directory == current_dir || project.directory.ends_with(&current_dir) {
            if let Some(svc) = project.services.get(&args.service) {
                return Ok((name.clone(), svc.container_name.clone(), args.service.clone()));
            }
            let available: Vec<&String> = project.services.keys().collect();
            anyhow::bail!(
                "Service '{}' not found. Available: {}",
                args.service,
                available.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            );
        }
    }

    let container = containers
        .iter()
        .find(|c| c.service == args.service)
        .with_context(|| {
            let names: Vec<&str> = containers.iter().map(|c| c.service.as_str()).collect();
            format!(
                "No running project found for current directory. \
                 Available services across all projects: {}",
                names.join(", ")
            )
        })?;

    let project_name = containers
        .iter()
        .find(|c| c.id == container.id)
        .map(|c| project_name_from_container(&conduit_state, &c.id))
        .unwrap_or_default();

    Ok((project_name, container.name.clone(), container.service.clone()))
}

fn project_name_from_container(state: &state::ConduitState, container_id: &str) -> String {
    for (name, project) in &state.projects {
        for svc in project.services.values() {
            if svc.container_id == container_id {
                return name.clone();
            }
        }
    }
    String::new()
}
