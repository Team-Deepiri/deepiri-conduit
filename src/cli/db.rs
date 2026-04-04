use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::cli::GlobalOpts;
use crate::config;
use crate::config::conduit_yml::DatabaseConfig;
use crate::docker;
use crate::registry::state;
use crate::tunnel::tcp::{self, DbType, TcpTunnel};

#[derive(Args)]
pub struct DbArgs {
    /// Service name (e.g., postgres, redis, mongo)
    pub service: String,

    /// Use a specific host port
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Target project (default: current directory)
    #[arg(long)]
    pub project: Option<String>,
}

pub async fn run(args: DbArgs, cli: &GlobalOpts) -> Result<()> {
    let global_config = config::load_global_config()?;
    let docker = docker::client::connect().await?;

    let project_name = match &args.project {
        Some(p) => p.clone(),
        None => {
            let project_dir = match &cli.project_dir {
                Some(dir) => PathBuf::from(dir),
                None => std::env::current_dir()?,
            };
            let project_config = config::load_project_config(&project_dir)?;
            project_config.project.unwrap_or_else(|| {
                project_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "default".to_string())
            })
        }
    };

    let conduit_state = state::load()?;
    let project_state = conduit_state.projects.get(&project_name).with_context(|| {
        format!(
            "Project '{}' is not running. Run `conduit up` first.",
            project_name
        )
    })?;

    let svc_state = project_state.services.get(&args.service).with_context(|| {
        let available: Vec<&String> = project_state.services.keys().collect();
        format!(
            "Service '{}' not found in project '{}'. Available: {}",
            args.service,
            project_name,
            available
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    let db_type = tcp::detect_db_type(&svc_state.image).with_context(|| {
        format!(
            "Cannot detect database type from image '{}'. Is '{}' a database service?",
            svc_state.image, args.service
        )
    })?;

    let project_dir = PathBuf::from(&project_state.directory);
    let conduit_yaml = config::load_project_config(&project_dir).unwrap_or_default();
    let db_cfg = conduit_yaml
        .databases
        .as_ref()
        .and_then(|m| m.get(&args.service));

    let env = docker::container::container_env_map(&docker, &svc_state.container_id).await?;

    let container_port = db_cfg
        .and_then(|c| c.port)
        .unwrap_or_else(|| db_type.default_port());

    let (user, password, database) = resolve_credentials(&db_type, &env, db_cfg);

    let container_ip =
        docker::network::get_container_ip(&docker, &svc_state.container_id, &project_state.network)
            .await
            .with_context(|| {
                format!(
                    "Cannot find IP for container {}. Is it running?",
                    svc_state.container_name
                )
            })?;

    let port_range = match db_type {
        DbType::PostgreSQL => global_config.tunnels.postgres_range,
        DbType::MongoDB => global_config.tunnels.mongodb_range,
        DbType::Redis => global_config.tunnels.redis_range,
        DbType::MySQL => global_config.tunnels.mysql_range,
        DbType::ClickHouse => global_config.tunnels.default_range,
    };

    let tunnel = TcpTunnel::start(&container_ip, container_port, args.port, port_range).await?;

    let conn_str =
        db_type.connection_string("localhost", tunnel.host_port, &user, &password, &database);
    let cli_cmd = db_type.cli_command("localhost", tunnel.host_port, &user, &database);

    println!();
    println!(
        "  {} Detected: {} ({})",
        "✓".green(),
        db_type.name().bold(),
        svc_state.image
    );
    println!(
        "  {} Tunnel: localhost:{} → {}:{}",
        "✓".green(),
        tunnel.host_port.to_string().cyan(),
        svc_state.container_name,
        container_port
    );
    println!();
    println!("  ┌─────────────────────────────────────────────────────────┐");
    println!("  │  Connection String:                                     │");
    println!("  │  {}  │", conn_str.cyan());
    println!("  │                                                         │");
    println!("  │  CLI:                                                   │");
    println!("  │  {}  │", cli_cmd.cyan());
    println!("  └─────────────────────────────────────────────────────────┘");
    println!();
    println!("  Tunnel active. Press Ctrl+C to close.");

    tokio::signal::ctrl_c().await?;

    println!(
        "\n  {} Tunnel closed ({} total connections)",
        "✓".green(),
        tunnel
            .total_connections
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    tunnel.stop();

    Ok(())
}

fn resolve_credentials(
    db_type: &DbType,
    env: &HashMap<String, String>,
    db_cfg: Option<&DatabaseConfig>,
) -> (String, String, String) {
    let get = |key: &str| env.get(key).cloned().unwrap_or_default();

    if let Some(cfg) = db_cfg {
        let u = cfg.user_env.as_deref().map(get).unwrap_or_default();
        let p = cfg.password_env.as_deref().map(get).unwrap_or_default();
        let d = cfg.database_env.as_deref().map(get).unwrap_or_default();
        if !u.is_empty() || !p.is_empty() || !d.is_empty() {
            return (u, p, d);
        }
    }

    match db_type {
        DbType::PostgreSQL => (
            none_then(&get("POSTGRES_USER"), "postgres"),
            get("POSTGRES_PASSWORD"),
            none_then(&get("POSTGRES_DB"), "postgres"),
        ),
        DbType::MySQL => {
            let pw = {
                let r = get("MYSQL_ROOT_PASSWORD");
                if !r.is_empty() {
                    r
                } else {
                    get("MYSQL_PASSWORD")
                }
            };
            (
                none_then(&get("MYSQL_USER"), "root"),
                pw,
                none_then(&get("MYSQL_DATABASE"), "mysql"),
            )
        }
        DbType::MongoDB => (
            none_then(&get("MONGO_INITDB_ROOT_USERNAME"), "root"),
            get("MONGO_INITDB_ROOT_PASSWORD"),
            "admin".into(),
        ),
        DbType::Redis => (String::new(), get("REDIS_PASSWORD"), String::new()),
        DbType::ClickHouse => (
            none_then(&get("CLICKHOUSE_USER"), "default"),
            get("CLICKHOUSE_PASSWORD"),
            none_then(&get("CLICKHOUSE_DB"), "default"),
        ),
    }
}

fn none_then(s: &str, default: &str) -> String {
    if s.is_empty() {
        default.to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn postgres_env_heuristic() {
        let mut env = HashMap::new();
        env.insert("POSTGRES_USER".into(), "app".into());
        env.insert("POSTGRES_PASSWORD".into(), "secret".into());
        env.insert("POSTGRES_DB".into(), "mydb".into());
        let (u, p, d) = resolve_credentials(&DbType::PostgreSQL, &env, None);
        assert_eq!(u, "app");
        assert_eq!(p, "secret");
        assert_eq!(d, "mydb");
    }

    #[test]
    fn conduit_yml_overrides() {
        let cfg = DatabaseConfig {
            db_type: "postgresql".into(),
            user_env: Some("MY_USER".into()),
            password_env: Some("MY_PASS".into()),
            database_env: Some("MY_DB".into()),
            port: None,
        };
        let mut env2 = HashMap::new();
        env2.insert("MY_USER".into(), "u".into());
        env2.insert("MY_PASS".into(), "p".into());
        env2.insert("MY_DB".into(), "d".into());
        let (u, p, d) = resolve_credentials(&DbType::PostgreSQL, &env2, Some(&cfg));
        assert_eq!((u, p, d), ("u".into(), "p".into(), "d".into()));
    }
}
