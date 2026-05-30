use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::compose::parser;
use crate::config;
use crate::registry::state;

#[derive(Args)]
pub struct GraphArgs {
    /// Service to highlight (or show full tree)
    #[arg(short, long)]
    pub service: Option<String>,

    /// Output format (ascii, mermaid)
    #[arg(long, default_value = "ascii")]
    pub format: String,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,
}

pub async fn run(args: GraphArgs, cli: &GlobalOpts) -> Result<()> {
    let conduit_state = state::load()?;

    let project_name = if let Some(name) = &args.project {
        name.clone()
    } else {
        let current_dir = cli.project_dir.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        });
        let mut found = None;
        for (name, project) in &conduit_state.projects {
            if project.directory == current_dir || project.directory.ends_with(&current_dir) {
                found = Some(name.clone());
                break;
            }
        }
        found.with_context(|| {
            "No running project found for current directory. Use --project or run `conduit up` first."
        })?
    };

    let project = conduit_state
        .projects
        .get(&project_name)
        .with_context(|| format!("Project '{}' not found in state", project_name))?;

    let project_dir = std::path::PathBuf::from(&project.directory);
    let project_config = config::load_project_config(&project_dir).unwrap_or_default();
    let compose_path = find_compose(&project_config, &project_dir)?;

    let compose = parser::parse(&compose_path)?;

    println!();
    println!("  {} Dependency graph for {}", "Graph".bold(), project_name.bold());
    println!();

    let services: Vec<&String> = compose.services.keys().collect();

    if args.format == "mermaid" {
        print_mermaid(&compose, &services, &args.service);
    } else {
        print_ascii(&compose, &services, &args.service, 0, &mut std::collections::HashSet::new());
    }

    println!();
    Ok(())
}

fn print_ascii(
    compose: &crate::compose::types::ComposeFile,
    services: &[&String],
    highlight: &Option<String>,
    depth: usize,
    visited: &mut std::collections::HashSet<String>,
) {
    let indent = "  ".repeat(depth);

    for svc_name in services {
        if !visited.insert((*svc_name).clone()) {
            continue;
        }

        let is_highlighted = highlight.as_ref().map_or(false, |h| h == *svc_name);
        let label = if is_highlighted {
            svc_name.bold().green().to_string()
        } else {
            svc_name.normal().to_string()
        };

        if depth == 0 {
            println!("{}{}", indent, label);
        } else {
            println!("{}└─ {}", indent, label);
        }

        if let Some(svc) = compose.services.get(*svc_name) {
            if let Some(dep) = &svc.depends_on {
                let dep_names = dep.service_names();
                let dep_refs: Vec<&String> = dep_names.iter().filter(|d| compose.services.contains_key(*d)).collect();
                if !dep_refs.is_empty() {
                    print_ascii(compose, &dep_refs, highlight, depth + 1, visited);
                }
            }
        }
    }
}

fn print_mermaid(
    compose: &crate::compose::types::ComposeFile,
    services: &[&String],
    _highlight: &Option<String>,
) {
    println!("  ```mermaid");
    println!("  graph TD;");
    for svc_name in services {
        if let Some(svc) = compose.services.get(*svc_name) {
            if let Some(dep) = &svc.depends_on {
                for dep_name in dep.service_names() {
                    if compose.services.contains_key(&dep_name) {
                        println!("    {} --> {};", dep_name, svc_name);
                    }
                }
            }
        }
    }
    println!("  ```");
    println!();
    println!("  Copy the mermaid block above into https://mermaid.live to visualize.");
}

fn find_compose(
    project_config: &config::ConduitConfig,
    project_dir: &std::path::Path,
) -> Result<std::path::PathBuf> {
    if let Some(file) = &project_config.compose_file {
        let path = project_dir.join(file);
        if path.exists() {
            return Ok(path);
        }
    }
    parser::find_compose_file(project_dir)
        .context("No compose file found in project directory")
}
