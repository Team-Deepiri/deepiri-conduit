use anyhow::Result;
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;
use crate::config;

#[derive(Args)]
pub struct ConfigCmdArgs {
    /// Show global config
    #[arg(long)]
    pub global: bool,

    /// Show project config
    #[arg(long)]
    pub project: bool,
}

pub async fn run(args: ConfigCmdArgs, cli: &GlobalOpts) -> Result<()> {
    if args.global {
        let global = config::load_global_config()?;
        let toml_str = toml::to_string_pretty(&global)?;
        println!(
            "\n  {} (~/.config/conduit/config.toml)\n",
            "Global Config".bold()
        );
        for line in toml_str.lines() {
            println!("  {}", line);
        }
        println!();
        return Ok(());
    }

    let project_dir = match &cli.project_dir {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::env::current_dir()?,
    };

    if args.project || !args.global {
        let project_cfg = config::load_project_config(&project_dir)?;
        let yaml = serde_yaml::to_string(&project_cfg)?;
        println!("\n  {} (.conduit.yml)\n", "Project Config".bold());
        for line in yaml.lines() {
            println!("  {}", line);
        }
        println!();
    }

    Ok(())
}
