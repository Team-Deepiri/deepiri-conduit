use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use std::time::Instant;

use crate::cli::GlobalOpts;
use crate::registry::state;

#[derive(Args)]
pub struct BenchArgs {
    /// Specific route to bench (default: all routes)
    #[arg(short, long)]
    pub route: Option<String>,

    /// Number of requests per endpoint
    #[arg(short, long, default_value = "5")]
    pub count: u32,

    /// Concurrency level
    #[arg(short, long, default_value = "1")]
    pub concurrency: u32,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,

    /// Timeout per request in seconds
    #[arg(long, default_value = "5")]
    pub timeout: u64,
}

pub async fn run(args: BenchArgs, _cli: &GlobalOpts) -> Result<()> {
    let conduit_state = state::load()?;

    let routes = if let Some(single_route) = &args.route {
        vec![(format!("custom"), single_route.clone())]
    } else {
        let mut all_routes = Vec::new();
        for (_name, project) in &conduit_state.projects {
            if let Some(ref project_name) = args.project {
                let name = if let Some((name, _)) = conduit_state.projects.iter().find(|(n, p)| {
                    *n == project_name || p.directory.ends_with(project_name)
                }) {
                    name.clone()
                } else {
                    continue;
                };
                if name != *project_name {
                    continue;
                }
            }
            for (domain, target) in &project.routes {
                all_routes.push((format!("http://{}/", domain), target.clone()));
            }
        }
        if all_routes.is_empty() {
            anyhow::bail!(
                "No routes found. Are any conduit projects running?"
            );
        }
        all_routes
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(args.timeout))
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("Failed to create HTTP client")?;

    println!();
    println!(
        "  {} {}",
        "Benchmarking".bold(),
        routes
            .iter()
            .map(|(u, _)| u.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("  {} requests per endpoint, concurrency {}", args.count, args.concurrency);
    println!();

    for (url, target) in &routes {
        let mut total_duration = std::time::Duration::from_secs(0);
        let mut successes = 0u32;
        let mut failures = 0u32;
        let mut min_duration = std::time::Duration::MAX;
        let mut max_duration = std::time::Duration::from_secs(0);

        print!("  {} {} → {} ...", "GET".cyan(), url.cyan(), target.cyan());

        for _ in 0..args.count {
            let start = Instant::now();
            match client.get(url).send().await {
                Ok(_resp) => {
                    let elapsed = start.elapsed();
                    total_duration += elapsed;
                    successes += 1;
                    min_duration = min_duration.min(elapsed);
                    max_duration = max_duration.max(elapsed);
                }
                Err(_e) => {
                    failures += 1;
                }
            }
        }

        let avg = if successes > 0 {
            total_duration / successes
        } else {
            std::time::Duration::from_secs(0)
        };

        let status_str = if successes == args.count {
            "✓".green()
        } else if successes > 0 {
            "⚠".yellow()
        } else {
            "✗".red()
        };

        println!(
            " {} {} OK, {} fail (min: {:?}, avg: {:?}, max: {:?})",
            status_str,
            successes,
            failures,
            min_duration,
            avg,
            max_duration,
        );
    }

    println!();
    Ok(())
}
