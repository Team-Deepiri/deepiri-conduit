use anyhow::Result;
use clap::Args;
use colored::Colorize;
use futures_util::StreamExt;
use std::time::Duration;

use crate::cli::GlobalOpts;
use crate::docker;
use crate::registry::state;

#[derive(Args)]
pub struct TopArgs {
    /// Target project (default: all projects)
    #[arg(short, long)]
    pub project: Option<String>,

    /// Refresh interval in seconds
    #[arg(short, long, default_value = "2")]
    pub interval: u64,

    /// Number of refreshes (0 = infinite)
    #[arg(short = 'n', long, default_value = "0")]
    pub count: u64,

    /// One-shot view (no continuous polling)
    #[arg(long)]
    pub once: bool,

    /// JSON output
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Default)]
struct ContainerStats {
    cpu_percent: f64,
    memory_usage_mb: f64,
    memory_limit_mb: f64,
    memory_percent: f64,
    network_rx_mb: f64,
    network_tx_mb: f64,
    pids: u64,
    block_read_mb: f64,
    block_write_mb: f64,
}

pub async fn run(args: TopArgs, _cli: &GlobalOpts) -> Result<()> {
    let docker = docker::client::connect().await?;
    let conduit_state = state::load()?;

    let mut target_containers: Vec<(String, String, String)> = Vec::new();

    for (name, project) in &conduit_state.projects {
        if let Some(ref filter) = args.project {
            if name != filter && !project.directory.ends_with(filter) {
                continue;
            }
        }
        for (svc_name, svc) in &project.services {
            if svc.status == "running" {
                target_containers.push((name.clone(), svc_name.clone(), svc.container_id.clone()));
            }
        }
    }

    if target_containers.is_empty() {
        println!("  No running containers found.");
        return Ok(());
    }

    let mut iteration = 0u64;
    let max_iterations = if args.count > 0 { args.count } else { u64::MAX };

    loop {
        if iteration >= max_iterations {
            break;
        }
        iteration += 1;

        if args.once {
            print_stats(&docker, &target_containers, &args, true).await?;
            break;
        }

        print_stats(&docker, &target_containers, &args, false).await?;

        tokio::time::sleep(Duration::from_secs(args.interval)).await;
    }

    Ok(())
}

async fn print_stats(
    docker: &bollard::Docker,
    targets: &[(String, String, String)],
    args: &TopArgs,
    is_once: bool,
) -> Result<()> {
    let mut all_stats: Vec<(String, String, ContainerStats)> = Vec::new();

    for (project, service, container_id) in targets {
        let stats = get_container_stats(docker, container_id).await;
        match stats {
            Ok(s) => {
                all_stats.push((project.clone(), service.clone(), s));
            }
            Err(_) => {
                all_stats.push((
                    project.clone(),
                    service.clone(),
                    ContainerStats {
                        cpu_percent: 0.0,
                        memory_usage_mb: 0.0,
                        memory_limit_mb: 0.0,
                        memory_percent: 0.0,
                        network_rx_mb: 0.0,
                        network_tx_mb: 0.0,
                        pids: 0,
                        block_read_mb: 0.0,
                        block_write_mb: 0.0,
                    },
                ));
            }
        }
    }

    if args.json {
        #[derive(serde::Serialize)]
        struct StatEntry {
            project: String,
            service: String,
            cpu_percent: f64,
            memory_usage_mb: f64,
            memory_percent: f64,
            pids: u64,
        }
        let entries: Vec<StatEntry> = all_stats
            .iter()
            .map(|(p, s, st)| StatEntry {
                project: p.clone(),
                service: s.clone(),
                cpu_percent: st.cpu_percent,
                memory_usage_mb: st.memory_usage_mb,
                memory_percent: st.memory_percent,
                pids: st.pids,
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    if !is_once {
        print!("\x1B[2J\x1B[H");
    }

    println!(
        "  {:<18} {:<22} {:>8} {:>12} {:>12} {:>6}",
        "PROJECT".bold(),
        "SERVICE".bold(),
        "CPU %".bold(),
        "MEM USAGE".bold(),
        "MEM %".bold(),
        "PIDS".bold(),
    );
    println!("  {}", "─".repeat(85));

    for (project, service, stats) in &all_stats {
        let cpu_str = format!("{:.1}%", stats.cpu_percent);
        let cpu_colored = if stats.cpu_percent > 80.0 {
            cpu_str.red()
        } else if stats.cpu_percent > 50.0 {
            cpu_str.yellow()
        } else {
            cpu_str.green()
        };

        let mem_str = format!("{:.1}MB", stats.memory_usage_mb);
        let mem_colored = if stats.memory_percent > 80.0 {
            mem_str.red()
        } else if stats.memory_percent > 50.0 {
            mem_str.yellow()
        } else {
            mem_str.green()
        };

        let pct_colored = if stats.memory_percent > 80.0 {
            format!("{:.1}%", stats.memory_percent).red()
        } else if stats.memory_percent > 50.0 {
            format!("{:.1}%", stats.memory_percent).yellow()
        } else {
            format!("{:.1}%", stats.memory_percent).green()
        };

        println!(
            "  {:<18} {:<22} {:>8} {:>12} {:>12} {:>6}",
            project,
            service,
            cpu_colored,
            mem_colored,
            pct_colored,
            stats.pids,
        );
    }

    if is_once {
        println!();
        println!("  Use {} without {} for live refresh", "conduit top", "--once".cyan());
    }

    Ok(())
}

async fn get_container_stats(
    docker: &bollard::Docker,
    container_id: &str,
) -> Result<ContainerStats> {
    use bollard::container::StatsOptions;

    let options = StatsOptions {
        stream: false,
        one_shot: true,
    };

    let mut stream = docker.stats(container_id, Some(options));
    let item = stream.next().await;

    match item {
        Some(Ok(stats)) => {
            let cpu_delta = stats.cpu_stats.cpu_usage.total_usage as f64
                - stats.precpu_stats.cpu_usage.total_usage as f64;
            let system_delta = stats.cpu_stats.system_cpu_usage.unwrap_or(0) as f64
                - stats.precpu_stats.system_cpu_usage.unwrap_or(0) as f64;
            let num_cpus = stats.cpu_stats.online_cpus.unwrap_or(1) as f64;

            let cpu_percent = if system_delta > 0.0 && cpu_delta > 0.0 {
                (cpu_delta / system_delta) * num_cpus * 100.0
            } else {
                0.0
            };

            let memory_usage = stats.memory_stats.usage.unwrap_or(0) as f64;
            let memory_limit = stats.memory_stats.limit.unwrap_or(1) as f64;
            let memory_percent = if memory_limit > 0.0 {
                (memory_usage / memory_limit) * 100.0
            } else {
                0.0
            };

            let pids = stats.pids_stats.current.unwrap_or(0) as u64;

            let mut network_rx = 0u64;
            let mut network_tx = 0u64;
            if let Some(networks) = &stats.networks {
                for (_name, net) in networks {
                    network_rx += net.rx_bytes;
                    network_tx += net.tx_bytes;
                }
            }

            let mut block_read = 0u64;
            let mut block_write = 0u64;
            if let Some(ref services) = stats.blkio_stats.io_service_bytes_recursive {
                for svc in services {
                    let op = svc.op.as_str();
                    let val = svc.value;
                    if op.contains("read") || op == "Read" {
                        block_read += val;
                    } else if op.contains("write") || op == "Write" {
                        block_write += val;
                    }
                }
            }

            Ok(ContainerStats {
                cpu_percent,
                memory_usage_mb: memory_usage / 1_000_000.0,
                memory_limit_mb: memory_limit / 1_000_000.0,
                memory_percent,
                network_rx_mb: network_rx as f64 / 1_000_000.0,
                network_tx_mb: network_tx as f64 / 1_000_000.0,
                pids,
                block_read_mb: block_read as f64 / 1_000_000.0,
                block_write_mb: block_write as f64 / 1_000_000.0,
            })
        }
        Some(Err(e)) => Err(e.into()),
        None => anyhow::bail!("No stats returned for container {}", container_id),
    }
}
