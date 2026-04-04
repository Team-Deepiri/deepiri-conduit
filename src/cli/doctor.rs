use anyhow::Result;
use colored::Colorize;
use std::net::TcpListener;

use crate::cli::GlobalOpts;
use crate::config::global;
use crate::dns;
use crate::docker;
use crate::proxy;
use crate::registry::state;

pub async fn run(_cli: &GlobalOpts) -> Result<()> {
    println!("\n  {}\n", "Conduit Doctor".bold());

    check_docker_env();
    check_docker().await;
    check_docker_compose().await;
    check_writable_dirs();
    check_port(80, "HTTP (Traefik `web` — proxy must bind here)");
    check_port(443, "HTTPS (optional; conduit uses HTTP-first routing)");
    check_hosts_file();
    check_proxy().await;
    check_state_file();
    check_wsl();

    println!();
    println!(
        "  {} If something fails: ensure Docker is running, port 80 is free for Traefik, and",
        "ℹ".blue()
    );
    println!("    `docker compose version` works. See README **Troubleshooting**.");
    println!();
    Ok(())
}

fn check_docker_env() {
    match std::env::var("DOCKER_HOST") {
        Ok(host) => println!("  {} DOCKER_HOST: {}", "ℹ".blue(), host.cyan()),
        Err(_) => println!("  {} DOCKER_HOST: (default unix socket)", "ℹ".blue()),
    }
}

async fn check_docker() {
    match docker::client::check_docker().await {
        Ok(info) => {
            println!(
                "  {} Docker Engine: v{} (API: {})",
                "✓".green(),
                info.version,
                info.api_version
            );
        }
        Err(e) => {
            println!(
                "  {} Docker Engine: {} — {}",
                "✗".red(),
                "not available".red(),
                e
            );
            println!(
                "      Hint: start Docker Desktop / dockerd, or set DOCKER_HOST for a remote daemon."
            );
        }
    }
}

async fn check_docker_compose() {
    match tokio::process::Command::new("docker")
        .args(["compose", "version"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("  {} Docker Compose: {}", "✓".green(), version.trim());
        }
        Ok(output) => {
            let err = String::from_utf8_lossy(&output.stderr);
            println!("  {} Docker Compose: failed — {}", "✗".red(), err.trim());
        }
        Err(e) => {
            println!("  {} Docker Compose: not runnable — {}", "✗".red(), e);
        }
    }
}

fn check_writable_dirs() {
    let state_dir = global::state_dir();
    let config_dir = global::config_dir();

    for (label, path) in [
        ("State", state_dir.as_path()),
        ("Config", config_dir.as_path()),
    ] {
        match std::fs::create_dir_all(path) {
            Ok(()) => {
                let probe = path.join(".conduit-write-test");
                match std::fs::write(&probe, b"ok") {
                    Ok(()) => {
                        let _ = std::fs::remove_file(&probe);
                        println!(
                            "  {} {} dir writable: {}",
                            "✓".green(),
                            label,
                            path.display()
                        );
                    }
                    Err(e) => {
                        println!(
                            "  {} {} dir not writable: {} — {}",
                            "✗".red(),
                            label,
                            path.display(),
                            e
                        );
                    }
                }
            }
            Err(e) => {
                println!(
                    "  {} {} dir: cannot create {} — {}",
                    "✗".red(),
                    label,
                    path.display(),
                    e
                );
            }
        }
    }
}

fn check_port(port: u16, label: &str) {
    match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(_) => {
            println!("  {} Port {}: available ({})", "✓".green(), port, label);
        }
        Err(_) => {
            println!(
                "  {} Port {}: {} ({})",
                "⚠".yellow(),
                port,
                "in use".yellow(),
                label
            );
        }
    }
}

fn check_hosts_file() {
    match dns::hosts::current_entries() {
        Ok(entries) if entries.is_empty() => {
            println!("  {} /etc/hosts: no conduit entries", "ℹ".blue());
        }
        Ok(entries) => {
            println!(
                "  {} /etc/hosts: {} conduit entr{}",
                "✓".green(),
                entries.len(),
                if entries.len() == 1 { "y" } else { "ies" }
            );
        }
        Err(e) => {
            println!(
                "  {} /etc/hosts: {} (sudo may be needed for DNS sync)",
                "⚠".yellow(),
                e
            );
        }
    }
}

async fn check_proxy() {
    match docker::client::connect().await {
        Ok(docker) => match proxy::manager::get_proxy_status(&docker).await {
            Ok(Some(status)) if status == "running" => {
                println!("  {} Conduit proxy: {}", "✓".green(), "running".green());
            }
            Ok(Some(status)) => {
                println!("  {} Conduit proxy: {}", "⚠".yellow(), status.yellow());
            }
            Ok(None) => {
                println!(
                    "  {} Conduit proxy: not created (expected until first `conduit up` with routes)",
                    "ℹ".blue()
                );
            }
            Err(e) => {
                println!("  {} Conduit proxy: error — {}", "⚠".yellow(), e);
            }
        },
        Err(_) => {
            println!(
                "  {} Conduit proxy: {} (Docker unavailable)",
                "✗".red(),
                "skipped".red()
            );
        }
    }
}

fn check_state_file() {
    match state::load() {
        Ok(s) => {
            let projects = s.projects.len();
            println!(
                "  {} State file: valid ({} project{})",
                "✓".green(),
                projects,
                if projects == 1 { "" } else { "s" }
            );
        }
        Err(e) => {
            println!("  {} State file: {}", "⚠".yellow(), e);
        }
    }
}

fn check_wsl() {
    let is_wsl = std::fs::read_to_string("/proc/version")
        .map(|v| v.to_lowercase().contains("microsoft"))
        .unwrap_or(false);

    if is_wsl {
        println!();
        println!("  {} WSL2 detected", "ℹ".blue());

        match std::process::Command::new("docker")
            .args(["info", "--format", "{{.OperatingSystem}}"])
            .output()
        {
            Ok(output) if output.status.success() => {
                let os = String::from_utf8_lossy(&output.stdout);
                println!(
                    "  {} Docker Desktop integration: {}",
                    "✓".green(),
                    os.trim()
                );
            }
            _ => {
                println!(
                    "  {} Docker Desktop integration: {}",
                    "⚠".yellow(),
                    "enable Docker Desktop → Settings → Resources → WSL integration".yellow()
                );
            }
        }
    }
}
