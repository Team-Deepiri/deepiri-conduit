use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use std::collections::BTreeMap;

use crate::cli::GlobalOpts;
use crate::compose::{parser, rewriter};
use crate::config::conduit_yml::{ConduitConfig, RouteConfig};

#[derive(Args)]
pub struct InitArgs {
    /// Source compose file
    #[arg(short, long)]
    pub file: Option<String>,

    /// Base domain
    #[arg(long)]
    pub domain: Option<String>,
}

pub async fn run(args: InitArgs, cli: &GlobalOpts) -> Result<()> {
    let project_dir = match &cli.project_dir {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::env::current_dir()?,
    };

    let config_path = project_dir.join(".conduit.yml");
    if config_path.exists() {
        anyhow::bail!(".conduit.yml already exists. Delete it first to re-initialize.");
    }

    let compose_path = match &args.file {
        Some(f) => project_dir.join(f),
        None => parser::find_compose_file(&project_dir)
            .context("No compose file found. Use --file to specify one.")?,
    };

    let compose = parser::parse(&compose_path)?;

    let project_name = compose
        .name
        .clone()
        .or_else(|| {
            project_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "project".to_string())
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "-");

    let domain_base = args
        .domain
        .unwrap_or_else(|| format!("{}.localhost", project_name));

    let mut routes = BTreeMap::new();

    for (name, service) in &compose.services {
        if rewriter::is_http_service(name, service) {
            if let Some(port) = service.guess_http_port() {
                routes.insert(
                    name.clone(),
                    RouteConfig {
                        domain: format!("{}.{}", name, domain_base),
                        port: Some(port),
                        websocket: None,
                    },
                );
            }
        }
    }

    let config = ConduitConfig {
        project: Some(project_name.clone()),
        compose_file: Some(
            compose_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        ),
        domain: Some(domain_base),
        routes: if routes.is_empty() {
            None
        } else {
            Some(routes)
        },
        groups: None,
        expose: None,
        env: None,
        health: None,
        databases: None,
    };

    let yaml = serde_yaml::to_string(&config)?;
    std::fs::write(&config_path, yaml)?;

    println!(
        "  {} Generated .conduit.yml ({} routes)",
        "✓".green(),
        config.routes.as_ref().map(|r| r.len()).unwrap_or(0)
    );
    println!("  Edit it to customize domains, groups, and expose rules.");

    Ok(())
}
