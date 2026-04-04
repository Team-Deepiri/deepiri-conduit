use anyhow::{Context, Result};
use std::net::TcpListener as StdTcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::copy_bidirectional;
use tokio::net::TcpListener;
use tracing::{debug, error, info};

/// A TCP tunnel that forwards connections from a local port to a container.
pub struct TcpTunnel {
    pub host_port: u16,
    pub target_addr: String,
    pub target_port: u16,
    pub active_connections: Arc<AtomicUsize>,
    pub total_connections: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
}

impl TcpTunnel {
    /// Start a TCP tunnel. Returns immediately; the tunnel runs in background tokio tasks.
    pub async fn start(
        target_ip: &str,
        target_port: u16,
        preferred_port: Option<u16>,
        port_range: [u16; 2],
    ) -> Result<Self> {
        let host_port = match preferred_port {
            Some(p) => {
                check_port_available(p)?;
                p
            }
            None => find_free_port(port_range)?,
        };

        let target_addr = format!("{}:{}", target_ip, target_port);
        let active = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));

        let listener = TcpListener::bind(format!("0.0.0.0:{}", host_port))
            .await
            .with_context(|| format!("Failed to bind to port {}", host_port))?;

        info!(
            "Tunnel listening on 0.0.0.0:{} → {}",
            host_port, target_addr
        );

        let active_clone = active.clone();
        let total_clone = total.clone();
        let shutdown_clone = shutdown.clone();
        let target = target_addr.clone();

        tokio::spawn(async move {
            loop {
                if shutdown_clone.load(Ordering::Relaxed) {
                    break;
                }
                match listener.accept().await {
                    Ok((client_stream, peer)) => {
                        debug!("Tunnel connection from {}", peer);
                        let target = target.clone();
                        let active = active_clone.clone();
                        let total = total_clone.clone();

                        tokio::spawn(async move {
                            active.fetch_add(1, Ordering::Relaxed);
                            total.fetch_add(1, Ordering::Relaxed);

                            match tokio::net::TcpStream::connect(&target).await {
                                Ok(mut server_stream) => {
                                    let mut client = client_stream;
                                    if let Err(e) =
                                        copy_bidirectional(&mut client, &mut server_stream).await
                                    {
                                        debug!("Tunnel connection ended: {}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to connect to {}: {}", target, e);
                                }
                            }

                            active.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) => {
                        error!("Tunnel accept error: {}", e);
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                }
            }
        });

        Ok(TcpTunnel {
            host_port,
            target_addr,
            target_port,
            active_connections: active,
            total_connections: total,
            shutdown,
        })
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

/// Find a free port within a range.
fn find_free_port(range: [u16; 2]) -> Result<u16> {
    for port in range[0]..=range[1] {
        if check_port_available(port).is_ok() {
            return Ok(port);
        }
    }
    anyhow::bail!("No free port found in range {}-{}", range[0], range[1])
}

fn check_port_available(port: u16) -> Result<()> {
    StdTcpListener::bind(format!("0.0.0.0:{}", port))
        .with_context(|| format!("Port {} is already in use", port))?;
    Ok(())
}

/// Detect database type from a Docker image name.
pub fn detect_db_type(image: &str) -> Option<DbType> {
    let image_lower = image.to_lowercase();
    if image_lower.contains("postgres") {
        Some(DbType::PostgreSQL)
    } else if image_lower.contains("mongo") {
        Some(DbType::MongoDB)
    } else if image_lower.contains("redis") {
        Some(DbType::Redis)
    } else if image_lower.contains("mysql") || image_lower.contains("mariadb") {
        Some(DbType::MySQL)
    } else if image_lower.contains("clickhouse") {
        Some(DbType::ClickHouse)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub enum DbType {
    PostgreSQL,
    MongoDB,
    Redis,
    MySQL,
    ClickHouse,
}

impl DbType {
    pub fn default_port(&self) -> u16 {
        match self {
            DbType::PostgreSQL => 5432,
            DbType::MongoDB => 27017,
            DbType::Redis => 6379,
            DbType::MySQL => 3306,
            DbType::ClickHouse => 9000,
        }
    }

    pub fn connection_string(
        &self,
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        database: &str,
    ) -> String {
        match self {
            DbType::PostgreSQL => {
                format!(
                    "postgresql://{}:{}@{}:{}/{}",
                    user, password, host, port, database
                )
            }
            DbType::MongoDB => {
                format!(
                    "mongodb://{}:{}@{}:{}/{}",
                    user, password, host, port, database
                )
            }
            DbType::Redis => {
                if password.is_empty() {
                    format!("redis://{}:{}", host, port)
                } else {
                    format!("redis://:{}@{}:{}", password, host, port)
                }
            }
            DbType::MySQL => {
                format!(
                    "mysql://{}:{}@{}:{}/{}",
                    user, password, host, port, database
                )
            }
            DbType::ClickHouse => {
                format!(
                    "clickhouse://{}:{}@{}:{}/{}",
                    user, password, host, port, database
                )
            }
        }
    }

    pub fn cli_command(&self, host: &str, port: u16, user: &str, database: &str) -> String {
        match self {
            DbType::PostgreSQL => {
                format!("psql -h {} -p {} -U {} -d {}", host, port, user, database)
            }
            DbType::MongoDB => {
                format!("mongosh --host {} --port {}", host, port)
            }
            DbType::Redis => {
                format!("redis-cli -h {} -p {}", host, port)
            }
            DbType::MySQL => {
                format!("mysql -h {} -P {} -u {}", host, port, user)
            }
            DbType::ClickHouse => {
                format!("clickhouse-client --host {} --port {}", host, port)
            }
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            DbType::PostgreSQL => "PostgreSQL",
            DbType::MongoDB => "MongoDB",
            DbType::Redis => "Redis",
            DbType::MySQL => "MySQL",
            DbType::ClickHouse => "ClickHouse",
        }
    }
}
