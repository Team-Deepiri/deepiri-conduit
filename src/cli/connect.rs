use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::cli::GlobalOpts;

#[derive(Args)]
pub struct ConnectArgs {
    #[command(subcommand)]
    pub command: ConnectCommand,
}

#[derive(Args)]
pub struct ConnectSshArgs {
    /// SSH connection string (e.g., user@host, user@host:port)
    pub destination: String,

    /// Local port for Docker API forwarding
    #[arg(long, default_value = "23750")]
    pub local_port: u16,

    /// Remote Docker socket path
    #[arg(long, default_value = "/var/run/docker.sock")]
    pub remote_socket: String,

    /// Identity file (private key)
    #[arg(short, long)]
    pub identity_file: Option<String>,

    /// Name for this connection (for disconnecting)
    #[arg(short, long)]
    pub name: Option<String>,

    /// Forward additional local ports (e.g., "8080:localhost:80")
    #[arg(short, long)]
    pub forward: Vec<String>,

    /// Run in background
    #[arg(long)]
    pub background: bool,
}

#[derive(Args)]
pub struct ConnectDisconnectArgs {
    /// Connection name or SSH destination
    pub name: Option<String>,
}

#[derive(Args)]
pub struct ConnectListArgs {
    /// Show all active connections
    #[arg(long)]
    pub verbose: bool,
}

#[derive(clap::Subcommand)]
pub enum ConnectCommand {
    /// Connect to a remote Docker host via SSH
    Ssh(ConnectSshArgs),
    /// Disconnect from a remote host
    Disconnect(ConnectDisconnectArgs),
    /// List active connections
    List(ConnectListArgs),
}

pub async fn run(args: ConnectArgs, _cli: &GlobalOpts) -> Result<()> {
    match args.command {
        ConnectCommand::Ssh(args) => connect_ssh(args).await,
        ConnectCommand::Disconnect(args) => disconnect(args).await,
        ConnectCommand::List(args) => list_connections(args).await,
    }
}

async fn connect_ssh(args: ConnectSshArgs) -> Result<()> {
    let connection_name = args
        .name
        .clone()
        .unwrap_or_else(|| args.destination.replace('@', "_").replace(':', "_"));

    let pid_file = get_pid_file(&connection_name);

    if pid_file.exists() {
        anyhow::bail!(
            "Connection '{}' already exists. Disconnect first with `conduit connect disconnect {}`.",
            connection_name,
            connection_name
        );
    }

    let local_port = args.local_port;
    let remote_socket = &args.remote_socket;

    let mut ssh_args = vec![
        "-NT".to_string(),
        "-L".to_string(),
        format!("{}:{}:{}", local_port, remote_socket, remote_socket),
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=30".to_string(),
    ];

    if let Some(ref identity) = args.identity_file {
        ssh_args.push("-i".to_string());
        ssh_args.push(identity.clone());
    }

    for forward in &args.forward {
        ssh_args.push("-L".to_string());
        ssh_args.push(forward.clone());
    }

    ssh_args.push(args.destination.clone());

    println!();
    println!(
        "  {} Connecting to remote Docker host {}",
        "→".cyan(),
        args.destination.bold()
    );
    println!(
        "  {} Forwarding Docker socket: localhost:{} → {}",
        "→".cyan(),
        local_port.to_string().cyan(),
        remote_socket.cyan()
    );

    if !args.forward.is_empty() {
        for fwd in &args.forward {
            println!("  {} Forwarding: {}", "→".cyan(), fwd.cyan());
        }
    }

    if args.background {
        let child = tokio::process::Command::new("ssh")
            .args(&ssh_args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn ssh process. Is SSH installed?")?;

        let pid = child.id();
        std::fs::create_dir_all(get_connections_dir())?;
        let connection_info = serde_json::json!({
            "name": connection_name,
            "destination": args.destination,
            "local_port": local_port,
            "pid": pid,
            "type": "ssh",
            "forwards": args.forward,
        });

        std::fs::write(
            &pid_file,
            serde_json::to_string_pretty(&connection_info)?,
        )?;

        println!(
            "  {} Tunnel established in background (PID: {})",
            "✓".green(),
            pid.unwrap_or(0)
        );
        println!(
            "  {} Set {} to use this remote Docker host",
            "ℹ".blue(),
            "DOCKER_HOST=tcp://localhost:23750".cyan()
        );
        println!();
        println!("  Use {} to disconnect", format!("conduit connect disconnect {}", connection_name).cyan());
    } else {
        println!();
        println!("  Connection active. Press Ctrl+C to close.");
        println!();
        println!(
            "  In another terminal, set: {}",
            format!("DOCKER_HOST=tcp://localhost:{}", local_port).cyan()
        );

        let mut child = tokio::process::Command::new("ssh")
            .args(&ssh_args)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .context("Failed to spawn ssh process. Is SSH installed?")?;

        let status = child.wait().await?;

        if !status.success() {
            anyhow::bail!("SSH connection failed with code {}", status);
        }
    }

    Ok(())
}

async fn disconnect(args: ConnectDisconnectArgs) -> Result<()> {
    let connections_dir = get_connections_dir();
    if !connections_dir.exists() {
        println!("  No active connections");
        return Ok(());
    }

    let name = match &args.name {
        Some(n) => n.clone(),
        None => {
            let entries = list_connection_files()?;
            if entries.is_empty() {
                println!("  No active connections");
                return Ok(());
            }
            if entries.len() == 1 {
                entries[0].clone()
            } else {
                println!("  Multiple connections. Specify which to disconnect:");
                for entry in &entries {
                    println!("    conduit connect disconnect {}", entry);
                }
                return Ok(());
            }
        }
    };

    let pid_file = get_pid_file(&name);
    if !pid_file.exists() {
        anyhow::bail!("Connection '{}' not found", name);
    }

    let contents = std::fs::read_to_string(&pid_file)?;
    let info: serde_json::Value = serde_json::from_str(&contents)?;
    let pid = info["pid"].as_u64().unwrap_or(0);

    if pid > 0 {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        println!("  {} Terminated tunnel (PID: {})", "✓".green(), pid);
    }

    std::fs::remove_file(&pid_file)?;
    println!("  {} Connection '{}' closed", "✓".green(), name.bold());

    Ok(())
}

async fn list_connections(args: ConnectListArgs) -> Result<()> {
    let entries = list_connection_files()?;

    if entries.is_empty() {
        println!("  No active connections");
        return Ok(());
    }

    println!();
    println!("  Active connections:");
    println!();

    for entry in &entries {
        let pid_file = get_pid_file(entry);
        if let Ok(contents) = std::fs::read_to_string(&pid_file) {
            if let Ok(info) = serde_json::from_str::<serde_json::Value>(&contents) {
                let name = info["name"].as_str().unwrap_or(entry);
                let dest = info["destination"].as_str().unwrap_or("?");
                let port = info["local_port"].as_u64().unwrap_or(0);
                let pid = info["pid"].as_u64().unwrap_or(0);

                if args.verbose {
                    println!("  {} {}", "•".cyan(), name.bold());
                    println!("    Destination:  {}", dest);
                    println!("    Local port:   {}", port);
                    println!("    PID:          {}", pid);
                    if let Some(forwards) = info["forwards"].as_array() {
                        if !forwards.is_empty() {
                            println!("    Forwards:");
                            for fwd in forwards {
                                println!("      {}", fwd.as_str().unwrap_or("?").cyan());
                            }
                        }
                    }
                    println!();
                } else {
                    println!(
                        "  {} {} → {} (localhost:{}, PID {})",
                        "•".cyan(),
                        name.bold(),
                        dest,
                        port,
                        pid
                    );
                }
            }
        }
    }

    Ok(())
}

fn get_connections_dir() -> std::path::PathBuf {
    let base = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.local/share"));
    base.join("conduit").join("connections")
}

fn get_pid_file(name: &str) -> std::path::PathBuf {
    get_connections_dir().join(format!("{}.json", name))
}

fn list_connection_files() -> Result<Vec<String>> {
    let dir = get_connections_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
            if let Some(name) = entry.path().file_stem() {
                names.push(name.to_string_lossy().to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}
