use anyhow::{Context, Result};
use chrono::Utc;
use clap::Args;
use colored::Colorize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cli::GlobalOpts;
use crate::compose::{emit, parser, rewriter};
use crate::config;
use crate::dns;
use crate::docker;
use crate::project_id;
use crate::proxy;
use crate::registry::state::{self, ProjectState, ServiceState};

#[derive(Args)]
pub struct UpArgs {
    /// Compose file path (auto-detected if not specified)
    #[arg(short, long)]
    pub file: Option<String>,

    /// Start only this service group (+ its dependencies)
    #[arg(short, long)]
    pub group: Option<String>,

    /// Docker Compose profile to activate
    #[arg(long)]
    pub profile: Option<String>,

    /// Skip proxy — keep host ports, no Traefik (still writes generated compose with conduit labels)
    #[arg(long)]
    pub no_proxy: bool,

    /// Force rebuild images before starting
    #[arg(long)]
    pub build: bool,

    /// Startup timeout per service in seconds
    #[arg(long, default_value = "120")]
    pub timeout: u64,
}

pub async fn run(args: UpArgs, cli: &GlobalOpts) -> Result<()> {
    let project_dir = resolve_project_dir(cli)?;
    let project_config = config::load_project_config(&project_dir)?;
    let global_config = config::load_global_config()?;

    let compose_path = resolve_compose_file(&args, &project_config, &project_dir)?;
    let project_name = project_config
        .project
        .clone()
        .or_else(|| {
            compose_path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "default".to_string());

    let compose_project = project_id::sanitize_compose_project(&project_name);

    println!(
        "  {} Parsing {}",
        "→".cyan(),
        compose_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    );

    let mut compose = parser::parse(&compose_path)?;

    if let Some(group_name) = &args.group {
        let group_services = project_config.resolve_group(group_name);
        if group_services.is_empty() {
            anyhow::bail!(
                "Group '{}' not found in .conduit.yml. Available groups: {}",
                group_name,
                project_config
                    .groups
                    .as_ref()
                    .map(|g| g.keys().cloned().collect::<Vec<_>>().join(", "))
                    .unwrap_or_else(|| "none".to_string())
            );
        }
        compose
            .services
            .retain(|name, _| group_services.contains(name));
        println!(
            "  {} Group '{}': {} services selected",
            "→".cyan(),
            group_name,
            compose.services.len()
        );
    }

    let service_count = compose.services.len();
    println!("  {} {} services found", "✓".green(), service_count);

    let rewrite_result = if args.no_proxy {
        rewriter::apply_conduit_labels_only(&mut compose, &project_name);
        rewriter::RewriteResult {
            routes: vec![],
            network_name: String::new(),
            stripped_ports: vec![],
        }
    } else {
        rewriter::rewrite(&mut compose, &project_config, &project_name)
    };

    if !args.no_proxy {
        let ports_stripped: usize = rewrite_result
            .stripped_ports
            .iter()
            .map(|(_, p)| p.len())
            .sum();
        if ports_stripped > 0 {
            println!(
                "  {} Stripped {} port binding{} (zero host exposure)",
                "✓".green(),
                ports_stripped,
                if ports_stripped == 1 { "" } else { "s" }
            );
        }
    }

    emit::write_generated(&project_dir, &compose)?;
    let generated_rel = emit::GENERATED_REL_PATH.to_string();

    println!("  {} Wrote {}", "→".cyan(), generated_rel.cyan());

    let docker = docker::client::connect().await?;

    if args.build {
        println!("  {} Building images...", "→".cyan());
        build_images(&project_dir, &generated_rel, &args).await?;
    }

    if !rewrite_result.network_name.is_empty() {
        println!(
            "  {} Creating network {}",
            "→".cyan(),
            rewrite_result.network_name
        );
        docker::network::create_network(&docker, &rewrite_result.network_name).await?;
    }

    if !args.no_proxy && !rewrite_result.routes.is_empty() {
        println!("  {} Ensuring proxy is running", "→".cyan());
        proxy::manager::ensure_running(&docker, &global_config).await?;
        proxy::manager::connect_to_project_network(&docker, &rewrite_result.network_name).await?;
    }

    println!(
        "  {} Starting containers (compose project: {})...",
        "→".cyan(),
        compose_project.bold()
    );
    start_containers_via_compose(&project_dir, &generated_rel, &compose_project, &args).await?;

    let mut project_state = ProjectState {
        directory: project_dir.to_string_lossy().to_string(),
        compose_file: compose_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        generated_compose: generated_rel.clone(),
        compose_project_name: compose_project.clone(),
        network: rewrite_result.network_name.clone(),
        started_at: Utc::now(),
        services: BTreeMap::new(),
        routes: BTreeMap::new(),
    };

    for route in &rewrite_result.routes {
        project_state.routes.insert(
            route.domain.clone(),
            format!("{}:{}", route.service_name, route.container_port),
        );
    }

    let containers = docker::container::list_project_containers(&docker, &project_name).await?;
    for c in &containers {
        let domain = rewrite_result
            .routes
            .iter()
            .find(|r| r.service_name == c.service)
            .map(|r| r.domain.clone());
        project_state.services.insert(
            c.service.clone(),
            ServiceState {
                container_id: c.id.clone(),
                container_name: c.name.clone(),
                image: c.image.clone(),
                status: c.state.clone(),
                domain,
            },
        );
    }

    let mut conduit_state = state::load()?;
    conduit_state
        .projects
        .insert(project_name.clone(), project_state);
    state::save(&conduit_state)?;

    if !args.no_proxy && !rewrite_result.routes.is_empty() {
        if let Err(e) = dns::hosts::sync_from_state(&conduit_state) {
            eprintln!(
                "  {} DNS sync failed: {}. Add domains to /etc/hosts manually if needed.",
                "⚠".yellow(),
                e
            );
        }
    }

    println!();
    if rewrite_result.routes.is_empty() {
        println!(
            "  {} Project {} is up ({} services)",
            "✓".green().bold(),
            project_name.bold(),
            service_count
        );
        if args.no_proxy {
            println!(
                "  {} Proxy skipped — services use compose port bindings.",
                "ℹ".blue()
            );
        }
    } else {
        println!("  {} Routes (HTTP via Traefik):", "✓".green().bold());
        for route in &rewrite_result.routes {
            let ws_indicator = if route.websocket { " (ws)" } else { "" };
            println!(
                "    http://{:<38} → {}:{}{}",
                route.domain.cyan(),
                route.service_name,
                route.container_port,
                ws_indicator
            );
        }
        println!();
        let default_domain = format!("{}.localhost", project_name);
        let domain_hint = project_config.domain.as_deref().unwrap_or(&default_domain);
        println!(
            "  {} Access via hostnames above (add /etc/hosts entries if needed). Base: {}",
            "ℹ".blue(),
            domain_hint
        );
        println!(
            "  {} Database tunnel: {}",
            "ℹ".blue(),
            "conduit db <service>".cyan()
        );
    }

    Ok(())
}

fn resolve_project_dir(cli: &GlobalOpts) -> Result<PathBuf> {
    match &cli.project_dir {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => std::env::current_dir().context("Failed to get current directory"),
    }
}

fn resolve_compose_file(
    args: &UpArgs,
    config: &config::ConduitConfig,
    project_dir: &Path,
) -> Result<PathBuf> {
    if let Some(file) = &args.file {
        let path = project_dir.join(file);
        if path.exists() {
            return Ok(path);
        }
        anyhow::bail!("Compose file not found: {}", path.display());
    }

    if let Some(file) = &config.compose_file {
        let path = project_dir.join(file);
        if path.exists() {
            return Ok(path);
        }
    }

    parser::find_compose_file(project_dir).context(
        "No docker-compose file found. Use --file or create a .conduit.yml with compose_file set.",
    )
}

async fn build_images(project_dir: &PathBuf, generated_rel: &str, _args: &UpArgs) -> Result<()> {
    let output = tokio::process::Command::new("docker")
        .args(["compose", "-f", generated_rel, "build"])
        .current_dir(project_dir)
        .output()
        .await
        .context("Failed to run docker compose build")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Image build failed:\n{}", stderr);
    }

    Ok(())
}

async fn start_containers_via_compose(
    project_dir: &PathBuf,
    generated_rel: &str,
    compose_project: &str,
    args: &UpArgs,
) -> Result<()> {
    let mut cmd_args = vec![
        "compose".to_string(),
        "-f".to_string(),
        generated_rel.to_string(),
        "-p".to_string(),
        compose_project.to_string(),
        "up".to_string(),
        "-d".to_string(),
        "--remove-orphans".to_string(),
    ];

    if let Some(profile) = &args.profile {
        // insert after "compose"
        cmd_args.insert(2, format!("--profile={}", profile));
    }

    let output = tokio::process::Command::new("docker")
        .args(&cmd_args)
        .current_dir(project_dir)
        .output()
        .await
        .context("Failed to run docker compose up")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!("docker compose up failed:\n{}\n{}", stderr, stdout);
    }

    Ok(())
}
