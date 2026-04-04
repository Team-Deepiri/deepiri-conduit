use anyhow::Result;
use clap::{Args, Subcommand};
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::config;
use crate::docker;
use crate::proxy;

#[derive(Args)]
pub struct ProxyCmdArgs {
    #[command(subcommand)]
    pub action: ProxyAction,
}

#[derive(Subcommand)]
pub enum ProxyAction {
    /// Show proxy status
    Status,
    /// Restart the proxy
    Restart,
    /// Stop the proxy
    Stop,
}

pub async fn run(args: ProxyCmdArgs, _cli: &GlobalOpts) -> Result<()> {
    let docker = docker::client::connect().await?;

    match args.action {
        ProxyAction::Status => match proxy::manager::get_proxy_status(&docker).await? {
            Some(status) if status == "running" => {
                let global = config::load_global_config()?;
                println!(
                    "  {} Proxy: {} ({})",
                    "✓".green(),
                    "running".green(),
                    global.proxy.image
                );
                println!("  HTTP:  :{}", global.proxy.http_port);
                println!("  HTTPS: :{}", global.proxy.https_port);
            }
            Some(status) => {
                println!("  {} Proxy: {}", "⚠".yellow(), status.yellow());
            }
            None => {
                println!("  {} Proxy: not running", "ℹ".blue());
            }
        },
        ProxyAction::Restart => {
            println!("  {} Stopping proxy...", "→".cyan());
            proxy::manager::stop(&docker).await?;
            let global = config::load_global_config()?;
            println!("  {} Starting proxy...", "→".cyan());
            proxy::manager::ensure_running(&docker, &global).await?;
            println!("  {} Proxy restarted", "✓".green());
        }
        ProxyAction::Stop => {
            proxy::manager::stop(&docker).await?;
            println!("  {} Proxy stopped", "✓".green());
        }
    }

    Ok(())
}
