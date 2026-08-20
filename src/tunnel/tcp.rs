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

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_db_type_by_image_name() {
        assert_eq!(detect_db_type("postgres:16"), Some(DbType::PostgreSQL));
        assert_eq!(detect_db_type("mongo:7"), Some(DbType::MongoDB));
        assert_eq!(detect_db_type("redis:7"), Some(DbType::Redis));
        assert_eq!(detect_db_type("mysql:8"), Some(DbType::MySQL));
        assert_eq!(detect_db_type("mariadb:11"), Some(DbType::MySQL));
        assert_eq!(detect_db_type("clickhouse:24"), Some(DbType::ClickHouse));
        assert_eq!(detect_db_type("nginx"), None);
        assert_eq!(detect_db_type(""), None);
    }

    #[test]
    fn detect_db_type_is_case_insensitive() {
        assert_eq!(detect_db_type("PostgreSQL"), Some(DbType::PostgreSQL));
        assert_eq!(detect_db_type("MONGO"), Some(DbType::MongoDB));
    }

    #[test]
    fn default_ports() {
        assert_eq!(DbType::PostgreSQL.default_port(), 5432);
        assert_eq!(DbType::MongoDB.default_port(), 27017);
        assert_eq!(DbType::Redis.default_port(), 6379);
        assert_eq!(DbType::MySQL.default_port(), 3306);
        assert_eq!(DbType::ClickHouse.default_port(), 9000);
    }

    #[test]
    fn connection_strings() {
        assert_eq!(
            DbType::PostgreSQL.connection_string("127.0.0.1", 54321, "dev", "secret", "app"),
            "postgresql://dev:secret@127.0.0.1:54321/app"
        );
        assert_eq!(
            DbType::MongoDB.connection_string("127.0.0.1", 27020, "dev", "secret", "app"),
            "mongodb://dev:secret@127.0.0.1:27020/app"
        );
        assert_eq!(
            DbType::Redis.connection_string("127.0.0.1", 63800, "", "", ""),
            "redis://127.0.0.1:63800"
        );
        assert_eq!(
            DbType::Redis.connection_string("127.0.0.1", 63800, "", "pw", ""),
            "redis://:pw@127.0.0.1:63800"
        );
        assert_eq!(
            DbType::MySQL.connection_string("127.0.0.1", 33060, "dev", "secret", "app"),
            "mysql://dev:secret@127.0.0.1:33060/app"
        );
        assert_eq!(
            DbType::ClickHouse.connection_string("127.0.0.1", 9000, "dev", "secret", "app"),
            "clickhouse://dev:secret@127.0.0.1:9000/app"
        );
    }

    #[test]
    fn cli_commands() {
        assert_eq!(
            DbType::PostgreSQL.cli_command("127.0.0.1", 54321, "dev", "app"),
            "psql -h 127.0.0.1 -p 54321 -U dev -d app"
        );
        assert_eq!(
            DbType::MongoDB.cli_command("127.0.0.1", 27020, "", ""),
            "mongosh --host 127.0.0.1 --port 27020"
        );
        assert_eq!(
            DbType::Redis.cli_command("127.0.0.1", 63800, "", ""),
            "redis-cli -h 127.0.0.1 -p 63800"
        );
        assert_eq!(
            DbType::MySQL.cli_command("127.0.0.1", 33060, "dev", "app"),
            "mysql -h 127.0.0.1 -P 33060 -u dev"
        );
        assert_eq!(
            DbType::ClickHouse.cli_command("127.0.0.1", 9000, "", ""),
            "clickhouse-client --host 127.0.0.1 --port 9000"
        );
    }

    #[test]
    fn display_names() {
        assert_eq!(DbType::PostgreSQL.name(), "PostgreSQL");
        assert_eq!(DbType::MongoDB.name(), "MongoDB");
        assert_eq!(DbType::Redis.name(), "Redis");
        assert_eq!(DbType::MySQL.name(), "MySQL");
        assert_eq!(DbType::ClickHouse.name(), "ClickHouse");
    }

    #[test]
    fn port_availability_checks() {
        let listener = StdTcpListener::bind("0.0.0.0:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(check_port_available(port).is_err());
        drop(listener);
        assert!(check_port_available(port).is_ok());
    }

    #[test]
    fn find_free_port_in_range() {
        let port = find_free_port([49200, 49299]).unwrap();
        assert!((49200..=49299).contains(&port));
    }
}
