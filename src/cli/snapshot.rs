use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use chrono::Utc;
use std::path::PathBuf;

use crate::cli::GlobalOpts;
use crate::config;
use crate::registry::state;

#[derive(Args)]
pub struct SnapshotArgs {
    #[command(subcommand)]
    pub command: SnapshotCommand,
}

#[derive(Args)]
pub struct SnapshotCreateArgs {
    /// Snapshot name (default: auto-generated)
    #[arg(short, long)]
    pub name: Option<String>,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct SnapshotListArgs {
    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct SnapshotRestoreArgs {
    /// Snapshot name to restore
    pub name: String,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct SnapshotDeleteArgs {
    /// Snapshot name to delete
    pub name: String,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(clap::Subcommand)]
pub enum SnapshotCommand {
    /// Create a snapshot of data volumes
    Create(SnapshotCreateArgs),
    /// List snapshots
    List(SnapshotListArgs),
    /// Restore from a snapshot
    Restore(SnapshotRestoreArgs),
    /// Delete a snapshot
    Delete(SnapshotDeleteArgs),
}

const SNAPSHOTS_SUBDIR: &str = ".conduit/snapshots";

pub async fn run(args: SnapshotArgs, cli: &GlobalOpts) -> Result<()> {
    match args.command {
        SnapshotCommand::Create(args) => create_snapshot(args, cli).await,
        SnapshotCommand::List(args) => list_snapshots(args, cli).await,
        SnapshotCommand::Restore(args) => restore_snapshot(args, cli).await,
        SnapshotCommand::Delete(args) => delete_snapshot(args, cli).await,
    }
}

fn get_project_dir(cli: &GlobalOpts, project_arg: &Option<String>) -> Result<PathBuf> {
    if let Some(name) = project_arg {
        let conduit_state = state::load()?;
        let project = conduit_state
            .projects
            .get(name)
            .with_context(|| format!("Project '{}' not found in state", name))?;
        return Ok(PathBuf::from(&project.directory));
    }
    if let Some(dir) = &cli.project_dir {
        return Ok(PathBuf::from(dir));
    }
    std::env::current_dir().context("Failed to get current directory")
}

fn snapshots_dir(project_dir: &PathBuf) -> PathBuf {
    project_dir.join(SNAPSHOTS_SUBDIR)
}

fn get_project_volumes(project_dir: &PathBuf) -> Result<Vec<String>> {
    let conduit_config = config::load_project_config(project_dir).unwrap_or_default();
    let compose_path = if let Some(file) = &conduit_config.compose_file {
        project_dir.join(file)
    } else {
        crate::compose::parser::find_compose_file(project_dir)
            .context("No compose file found")?
    };

    let compose = crate::compose::parser::parse(&compose_path)?;
    let mut volumes = Vec::new();

    if let Some(vol_config) = &compose.volumes {
        for (name, _config) in vol_config {
            volumes.push(name.clone());
        }
    }

    for (_svc_name, svc) in &compose.services {
        if let Some(vols) = &svc.volumes {
            for vol in vols {
                if let Some(vol_str) = vol.as_str() {
                    if let Some((first, _)) = vol_str.split_once(':') {
                        if !first.starts_with('.') && !first.starts_with('/') && !first.starts_with('~') {
                            if !volumes.contains(&first.to_string()) {
                                volumes.push(first.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(volumes)
}

async fn create_snapshot(args: SnapshotCreateArgs, cli: &GlobalOpts) -> Result<()> {
    let project_dir = get_project_dir(cli, &args.project)?;
    let snap_dir = snapshots_dir(&project_dir);
    std::fs::create_dir_all(&snap_dir)?;

    let snapshot_name = args
        .name
        .unwrap_or_else(|| format!("snap-{}", Utc::now().format("%Y%m%d-%H%M%S")));
    let snapshot_path = snap_dir.join(&snapshot_name);

    if snapshot_path.exists() {
        anyhow::bail!("Snapshot '{}' already exists", snapshot_name);
    }

    let project_name = project_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "default".to_string());

    let volumes = get_project_volumes(&project_dir)?;

    if volumes.is_empty() {
        anyhow::bail!(
            "No named volumes found for this project. Snapshot only works with named Docker volumes."
        );
    }

    std::fs::create_dir_all(&snapshot_path)?;

    for vol_name in &volumes {
        let full_vol_name = format!("{}_{}", project_name, vol_name);
        let tar_path = snapshot_path.join(format!("{}.tar.gz", vol_name));

        println!(
            "  {} Snapshotting volume {} → {}",
            "→".cyan(),
            full_vol_name.cyan(),
            tar_path.display().to_string().cyan()
        );

        let status = tokio::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "-v",
                &format!("{}:/source:ro", full_vol_name),
                "-v",
                &format!("{}:/dest", snapshot_path.display()),
                "alpine:latest",
                "tar",
                "czf",
                &format!("/dest/{}.tar.gz", vol_name),
                "-C",
                "/source",
                ".",
            ])
            .status()
            .await
            .with_context(|| format!("Failed to snapshot volume {}", full_vol_name))?;

        if !status.success() {
            anyhow::bail!("Failed to snapshot volume {}", full_vol_name);
        }
    }

    println!(
        "  {} Snapshot '{}' created ({} volumes)",
        "✓".green(),
        snapshot_name.bold(),
        volumes.len()
    );

    Ok(())
}

async fn list_snapshots(args: SnapshotListArgs, cli: &GlobalOpts) -> Result<()> {
    let project_dir = get_project_dir(cli, &args.project)?;
    let snap_dir = snapshots_dir(&project_dir);

    if !snap_dir.exists() {
        println!("  No snapshots found");
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(&snap_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        println!("  No snapshots found");
        return Ok(());
    }

    println!();
    println!("  Snapshots for {}", project_dir.display().to_string().cyan());
    println!();

    for entry in &entries {
        let name = entry.file_name().into_string().unwrap_or_default();
        let vol_count = std::fs::read_dir(entry.path())
            .map(|d| d.count())
            .unwrap_or(0);
        println!("  {} ({})", name.bold(), format!("{} volumes", vol_count).cyan());
    }

    println!();
    Ok(())
}

async fn restore_snapshot(args: SnapshotRestoreArgs, cli: &GlobalOpts) -> Result<()> {
    let project_dir = get_project_dir(cli, &args.project)?;
    let snapshot_path = snapshots_dir(&project_dir).join(&args.name);

    if !snapshot_path.exists() {
        anyhow::bail!("Snapshot '{}' not found at {}", args.name, snapshot_path.display());
    }

    let project_name = project_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "default".to_string());

    let tar_files: Vec<_> = std::fs::read_dir(&snapshot_path)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "gz").unwrap_or(false))
        .collect();

    if tar_files.is_empty() {
        anyhow::bail!("No volume data found in snapshot '{}'", args.name);
    }

    println!(
        "  {} Restoring from snapshot '{}'",
        "→".cyan(),
        args.name.bold()
    );

    for entry in &tar_files {
        let filename = entry.file_name().into_string().unwrap_or_default();
        let vol_name = filename.trim_end_matches(".tar.gz");
        let full_vol_name = format!("{}_{}", project_name, vol_name);
        let tar_path = entry.path();

        println!(
            "  {} Restoring volume {}",
            "→".cyan(),
            full_vol_name.cyan()
        );

        let status = tokio::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "-v",
                &format!("{}:/dest", full_vol_name),
                "-v",
                &format!("{}:/source/snapshot.tar.gz", tar_path.display()),
                "alpine:latest",
                "sh",
                "-c",
                "mkdir -p /dest && tar xzf /source/snapshot.tar.gz -C /dest",
            ])
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("Failed to restore volume {}", full_vol_name);
        }
    }

    println!(
        "  {} Snapshot '{}' restored ({} volumes)",
        "✓".green(),
        args.name.bold(),
        tar_files.len()
    );

    Ok(())
}

async fn delete_snapshot(args: SnapshotDeleteArgs, cli: &GlobalOpts) -> Result<()> {
    let project_dir = get_project_dir(cli, &args.project)?;
    let snapshot_path = snapshots_dir(&project_dir).join(&args.name);

    if !snapshot_path.exists() {
        anyhow::bail!("Snapshot '{}' not found", args.name);
    }

    std::fs::remove_dir_all(&snapshot_path)?;
    println!("  {} Snapshot '{}' deleted", "✓".green(), args.name.bold());

    Ok(())
}
