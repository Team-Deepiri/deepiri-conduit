use anyhow::Result;
use clap::Args;
use colored::Colorize;
use std::collections::HashMap;
use bollard::image::{ListImagesOptions, PruneImagesOptions};
use bollard::Docker;

use crate::cli::GlobalOpts;
use crate::docker;

#[derive(Args)]
pub struct ImageArgs {
    #[command(subcommand)]
    pub command: ImageCommand,
}

#[derive(Args)]
pub struct ImageListArgs {
    /// Show all images (not just conduit-managed)
    #[arg(short, long)]
    pub all: bool,

    /// Filter by repository name
    #[arg(short, long)]
    pub filter: Option<String>,

    /// JSON output
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct ImagePruneArgs {
    /// Force without confirmation
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args)]
pub struct ImagePullArgs {
    /// Image to pull (e.g., nginx:alpine)
    pub image: String,
}

#[derive(clap::Subcommand)]
pub enum ImageCommand {
    /// List images
    List(ImageListArgs),
    /// Prune unused images
    Prune(ImagePruneArgs),
    /// Pull an image
    Pull(ImagePullArgs),
}

pub async fn run(args: ImageArgs, _cli: &GlobalOpts) -> Result<()> {
    let docker = docker::client::connect().await?;

    match args.command {
        ImageCommand::List(args) => list_images(&docker, args).await,
        ImageCommand::Prune(args) => prune_images(&docker, args).await,
        ImageCommand::Pull(args) => pull_image(&docker, args).await,
    }
}

async fn list_images(docker: &Docker, args: ImageListArgs) -> Result<()> {
    let mut filters = HashMap::new();
    if !args.all {
        filters.insert("reference".to_string(), vec!["conduit*".to_string()]);
    }

    let options = ListImagesOptions {
        all: false,
        filters,
        ..Default::default()
    };

    let images = docker.list_images(Some(options)).await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&images)?);
        return Ok(());
    }

    if images.is_empty() {
        println!("  No images found");
        return Ok(());
    }

    println!();
    println!(
        "  {:<50} {:<15} {:<10}",
        "REPOSITORY".bold(),
        "TAG".bold(),
        "SIZE".bold(),
    );
    println!("  {}", "─".repeat(80));

    for img in &images {
        let repo_tags = &img.repo_tags;
        if repo_tags.is_empty() {
            continue;
        }
        for tag in repo_tags {
            if let Some(ref filter) = args.filter {
                if !tag.contains(filter) {
                    continue;
                }
            }
            let parts: Vec<&str> = tag.splitn(2, ':').collect();
            let repo = parts.first().unwrap_or(&"?");
            let tag_val = parts.get(1).unwrap_or(&"latest");
            let size_mb = img.size as f64 / 1_000_000.0;
            println!(
                "  {:<50} {:<15} {:.1} MB",
                repo.cyan(),
                tag_val,
                size_mb,
            );
        }
    }

    println!("  {} images total", images.len());
    println!();
    Ok(())
}

async fn prune_images(docker: &Docker, args: ImagePruneArgs) -> Result<()> {
    if !args.force {
        println!("  This will remove all unused images. Continue? [y/N] ");
        use std::io::{stdin, BufRead};
        let mut input = String::new();
        stdin().lock().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("  Aborted.");
            return Ok(());
        }
    }

    let options = PruneImagesOptions::<String> {
        ..Default::default()
    };

    let report = docker.prune_images(Some(options)).await?;
    let freed = report.space_reclaimed.unwrap_or(0);
    let freed_mb = freed as f64 / 1_000_000.0;

    println!(
        "  {} Pruned images — {:.1} MB reclaimed",
        "✓".green(),
        freed_mb
    );
    Ok(())
}

async fn pull_image(docker: &Docker, args: ImagePullArgs) -> Result<()> {
    use futures_util::StreamExt;
    use bollard::image::CreateImageOptions;

    let parts: Vec<&str> = args.image.splitn(2, ':').collect();
    let (from_image, tag) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        (args.image.as_str(), "latest")
    };

    println!("  {} Pulling {}:{} ...", "→".cyan(), from_image, tag);

    let mut stream = docker.create_image(
        Some(CreateImageOptions {
            from_image,
            tag,
            ..Default::default()
        }),
        None,
        None,
    );

    while let Some(result) = stream.next().await {
        match result {
            Ok(info) => {
                if let Some(status) = info.status {
                    println!("  {}", status);
                }
            }
            Err(e) => {
                eprintln!("  {} Pull error: {}", "⚠".yellow(), e);
            }
        }
    }

    println!("  {} Pull complete", "✓".green());
    Ok(())
}
