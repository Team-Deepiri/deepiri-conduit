use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use std::path::PathBuf;

use crate::cli::GlobalOpts;
use crate::config;
use crate::compose::parser;

#[derive(Args)]
pub struct ConfigValidateArgs {
    /// Path to .conduit.yml (default: auto-detect)
    #[arg(short, long)]
    pub file: Option<String>,

    /// Also validate the referenced compose file
    #[arg(long)]
    pub compose: bool,

    /// Strict mode — treat warnings as errors
    #[arg(long)]
    pub strict: bool,
}

pub async fn run(args: ConfigValidateArgs, cli: &GlobalOpts) -> Result<()> {
    let project_dir = resolve_project_dir(cli);
    let config_path = resolve_config_path(&args, &project_dir)?;

    println!();
    println!("  {} Validating {} ...", "Config".bold(), config_path.display().to_string().cyan());
    println!();

    let contents = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;

    let config: config::ConduitConfig = match serde_yaml::from_str(&contents) {
        Ok(c) => c,
        Err(e) => {
            println!("  {} YAML parse error: {}", "✗".red(), e);
            anyhow::bail!("Configuration file is invalid");
        }
    };

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if let Some(project) = &config.project {
        if project.is_empty() {
            errors.push("project name is empty".to_string());
        } else if !project.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
            warnings.push(format!(
                "project name '{}' contains special characters — may cause issues with Docker labels",
                project
            ));
        }
    }

    if let Some(compose_file) = &config.compose_file {
        let compose_path = project_dir.join(compose_file);
        if !compose_path.exists() {
            warnings.push(format!(
                "compose_file '{}' not found at '{}'",
                compose_file,
                compose_path.display()
            ));
        } else if args.compose {
            match parser::parse(&compose_path) {
                Ok(compose) => {
                    println!(
                        "  {} Compose file valid ({} services)",
                        "✓".green(),
                        compose.services.len()
                    );
                    validate_compose_consistency(&config, &compose, &mut errors, &mut warnings);
                }
                Err(e) => {
                    errors.push(format!("Failed to parse compose file: {}", e));
                }
            }
        }
    } else {
        let found = parser::find_compose_file(&project_dir);
        match found {
            Some(_) => warnings.push("no compose_file set — conduit will auto-detect".to_string()),
            None => warnings.push("no compose file found in project directory".to_string()),
        }
    }

    if let Some(groups) = &config.groups {
        for (name, group) in groups {
            if group.services.is_empty() {
                warnings.push(format!("group '{}' has no services", name));
            }
            if let Some(ref deps) = group.depends_on {
                for dep in deps {
                    if !groups.contains_key(dep) {
                        warnings.push(format!(
                            "group '{}' depends on '{}' which is not defined",
                            name, dep
                        ));
                    }
                }
            }
        }
    }

    if let Some(routes) = &config.routes {
        for (svc, route) in routes {
            if route.domain.is_empty() {
                errors.push(format!("route '{}' has empty domain", svc));
            }
        }
    }

    if let Some(expose) = &config.expose {
        for (svc, port) in expose {
            if *port == 0 {
                errors.push(format!("expose port for '{}' is invalid: {}", svc, port));
            } else if *port <= 1024 {
                warnings.push(format!("expose port {} for '{}' is privileged (<1024)", port, svc));
            }
        }
    }

    if !errors.is_empty() {
        for err in &errors {
            println!("  {} {}", "✗".red(), err);
        }
    }

    if !warnings.is_empty() {
        for warn in &warnings {
            let prefix = if args.strict { "✗".red() } else { "⚠".yellow() };
            println!("  {} {}", prefix, warn);
        }
    }

    if args.strict && !warnings.is_empty() {
        anyhow::bail!("{} warnings found (strict mode)", warnings.len());
    }

    if errors.is_empty() {
        println!(
            "  {} Configuration valid — {} checks passed",
            "✓".green(),
            config.groups.as_ref().map_or(0, |g| g.len())
                + config.routes.as_ref().map_or(0, |r| r.len())
                + if config.project.is_some() { 1 } else { 0 }
                + if config.compose_file.is_some() { 1 } else { 0 }
        );
    } else {
        anyhow::bail!("{} error(s) found in configuration", errors.len());
    }

    println!();
    Ok(())
}

fn validate_compose_consistency(
    config: &config::ConduitConfig,
    compose: &crate::compose::types::ComposeFile,
    _errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    if let Some(routes) = &config.routes {
        for svc in routes.keys() {
            if !compose.services.contains_key(svc) {
                warnings.push(format!(
                    "route defined for '{}' but no such service in compose file",
                    svc
                ));
            }
        }
    }

    if let Some(expose) = &config.expose {
        for svc in expose.keys() {
            if !compose.services.contains_key(svc) {
                warnings.push(format!(
                    "expose defined for '{}' but no such service in compose file",
                    svc
                ));
            }
        }
    }

    if let Some(databases) = &config.databases {
        for svc in databases.keys() {
            if !compose.services.contains_key(svc) {
                warnings.push(format!(
                    "database config for '{}' but no such service in compose file",
                    svc
                ));
            }
        }
    }
}

fn resolve_project_dir(cli: &GlobalOpts) -> PathBuf {
    match &cli.project_dir {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir().unwrap_or_default(),
    }
}

fn resolve_config_path(args: &ConfigValidateArgs, project_dir: &PathBuf) -> Result<PathBuf> {
    if let Some(file) = &args.file {
        let path = PathBuf::from(file);
        if path.is_absolute() {
            return Ok(path);
        }
        return Ok(project_dir.join(path));
    }
    let path = project_dir.join(".conduit.yml");
    if path.exists() {
        return Ok(path);
    }
    anyhow::bail!(
        "No .conduit.yml found in {}. Use --file to specify a path.",
        project_dir.display()
    );
}
