use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub dns: DnsConfig,
    #[serde(default)]
    pub tunnels: TunnelConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProxyConfig {
    #[serde(default = "default_proxy_image")]
    pub image: String,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default = "default_https_port")]
    pub https_port: u16,
    #[serde(default)]
    pub dashboard: bool,
    #[serde(default = "default_dashboard_port")]
    pub dashboard_port: u16,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            image: default_proxy_image(),
            http_port: default_http_port(),
            https_port: default_https_port(),
            dashboard: false,
            dashboard_port: default_dashboard_port(),
        }
    }
}

fn default_proxy_image() -> String {
    "traefik:v3.6".into()
}
fn default_http_port() -> u16 {
    80
}
fn default_https_port() -> u16 {
    443
}
fn default_dashboard_port() -> u16 {
    8080
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DnsConfig {
    #[serde(default = "default_dns_strategy")]
    pub strategy: String,
    #[serde(default = "default_true")]
    pub sudo: bool,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            strategy: default_dns_strategy(),
            sudo: true,
        }
    }
}

fn default_dns_strategy() -> String {
    "hosts".into()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TunnelConfig {
    #[serde(default = "default_pg_range")]
    pub postgres_range: [u16; 2],
    #[serde(default = "default_mongo_range")]
    pub mongodb_range: [u16; 2],
    #[serde(default = "default_redis_range")]
    pub redis_range: [u16; 2],
    #[serde(default = "default_mysql_range")]
    pub mysql_range: [u16; 2],
    #[serde(default = "default_range")]
    pub default_range: [u16; 2],
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            postgres_range: default_pg_range(),
            mongodb_range: default_mongo_range(),
            redis_range: default_redis_range(),
            mysql_range: default_mysql_range(),
            default_range: default_range(),
        }
    }
}

fn default_pg_range() -> [u16; 2] {
    [54320, 54399]
}
fn default_mongo_range() -> [u16; 2] {
    [27020, 27099]
}
fn default_redis_range() -> [u16; 2] {
    [63800, 63899]
}
fn default_mysql_range() -> [u16; 2] {
    [33060, 33099]
}
fn default_range() -> [u16; 2] {
    [49200, 49299]
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub file: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: String::new(),
        }
    }
}

fn default_log_level() -> String {
    "info".into()
}

pub fn config_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.config"))
        .join("conduit")
}

pub fn state_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.local/share"))
        .join("conduit")
}

pub fn load() -> Result<GlobalConfig> {
    let config_path = config_dir().join("config.toml");
    if config_path.exists() {
        let contents = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;
        let config: GlobalConfig = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse {}", config_path.display()))?;
        Ok(config)
    } else {
        Ok(GlobalConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let config = GlobalConfig::default();
        assert_eq!(config.proxy.image, "traefik:v3.6");
        assert_eq!(config.proxy.http_port, 80);
        assert_eq!(config.proxy.https_port, 443);
        assert!(!config.proxy.dashboard);
        assert_eq!(config.proxy.dashboard_port, 8080);
        assert_eq!(config.dns.strategy, "hosts");
        assert!(config.dns.sudo);
        assert_eq!(config.tunnels.postgres_range, [54320, 54399]);
        assert_eq!(config.tunnels.mongodb_range, [27020, 27099]);
        assert_eq!(config.tunnels.redis_range, [63800, 63899]);
        assert_eq!(config.tunnels.mysql_range, [33060, 33099]);
        assert_eq!(config.tunnels.default_range, [49200, 49299]);
        assert_eq!(config.logging.level, "info");
        assert!(config.logging.file.is_empty());
    }

    #[test]
    fn partial_toml_applies_defaults() {
        let config: GlobalConfig = toml::from_str("[proxy]\nhttp_port = 8080\n").unwrap();
        assert_eq!(config.proxy.http_port, 8080);
        assert_eq!(config.proxy.image, "traefik:v3.6");
        assert_eq!(config.dns.strategy, "hosts");
    }

    #[test]
    fn toml_roundtrip() {
        let config = GlobalConfig::default();
        let serialized = toml::to_string(&config).unwrap();
        let parsed: GlobalConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(parsed.proxy.image, config.proxy.image);
        assert_eq!(parsed.proxy.http_port, config.proxy.http_port);
        assert_eq!(parsed.tunnels.postgres_range, config.tunnels.postgres_range);
        assert_eq!(parsed.logging.level, config.logging.level);
    }

    #[test]
    fn dirs_point_at_conduit() {
        let dir = config_dir();
        assert_eq!(
            dir.file_name().map(|n| n.to_string_lossy().into_owned()),
            Some("conduit".into())
        );
        let dir = state_dir();
        assert_eq!(
            dir.file_name().map(|n| n.to_string_lossy().into_owned()),
            Some("conduit".into())
        );
    }
}
