use anyhow::Result;
use clap::Args;
use clap::CommandFactory;
use clap_complete::Shell;
use colored::Colorize;

use crate::cli::{Cli, GlobalOpts};

#[derive(Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

pub fn run(args: CompletionsArgs, _cli: &GlobalOpts) -> Result<()> {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();

    clap_complete::generate(args.shell, &mut cmd, bin_name, &mut std::io::stdout());

    let install_hint = match args.shell {
        Shell::Bash => "source <(conduit completions bash)  # add to ~/.bashrc".to_string(),
        Shell::Zsh => {
            "conduit completions zsh > \"${fpath[1]}/_conduit\"  # then: compinit".to_string()
        }
        Shell::Fish => {
            "conduit completions fish > ~/.config/fish/completions/conduit.fish".to_string()
        }
        _ => String::new(),
    };

    if !install_hint.is_empty() {
        eprintln!("\n  {} Install: {}", "ℹ".blue(), install_hint.dimmed());
    }
    Ok(())
}
