use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::docker;
use crate::registry::state;

#[derive(Args)]
pub struct LinkArgs {
    /// First project name
    pub project_a: String,
    /// Second project name
    pub project_b: String,
}

#[derive(Args)]
pub struct UnlinkArgs {
    /// First project name
    pub project_a: String,
    /// Second project name
    pub project_b: String,
}

pub async fn run_link(args: LinkArgs, _cli: &GlobalOpts) -> Result<()> {
    let docker = docker::client::connect().await?;
    let conduit_state = state::load()?;

    let proj_a = conduit_state
        .projects
        .get(&args.project_a)
        .with_context(|| format!("Project '{}' is not running", args.project_a))?;
    let proj_b = conduit_state
        .projects
        .get(&args.project_b)
        .with_context(|| format!("Project '{}' is not running", args.project_b))?;

    for svc in proj_a.services.values() {
        docker::network::connect_container(&docker, &proj_b.network, &svc.container_id)
            .await
            .ok();
    }
    for svc in proj_b.services.values() {
        docker::network::connect_container(&docker, &proj_a.network, &svc.container_id)
            .await
            .ok();
    }

    println!(
        "  {} Linked {} ↔ {}",
        "✓".green(),
        args.project_a.bold(),
        args.project_b.bold()
    );

    Ok(())
}

pub async fn run_unlink(args: UnlinkArgs, _cli: &GlobalOpts) -> Result<()> {
    let docker = docker::client::connect().await?;
    let conduit_state = state::load()?;

    let proj_a = conduit_state
        .projects
        .get(&args.project_a)
        .with_context(|| format!("Project '{}' is not running", args.project_a))?;
    let proj_b = conduit_state
        .projects
        .get(&args.project_b)
        .with_context(|| format!("Project '{}' is not running", args.project_b))?;

    for svc in proj_a.services.values() {
        docker::network::disconnect_container(&docker, &proj_b.network, &svc.container_id)
            .await
            .ok();
    }
    for svc in proj_b.services.values() {
        docker::network::disconnect_container(&docker, &proj_a.network, &svc.container_id)
            .await
            .ok();
    }

    println!(
        "  {} Unlinked {} ↔ {}",
        "✓".green(),
        args.project_a.bold(),
        args.project_b.bold()
    );

    Ok(())
}
