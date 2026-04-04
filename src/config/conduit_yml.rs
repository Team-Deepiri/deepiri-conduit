use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ConduitConfig {
    /// Project name (used for network naming, route prefixes)
    #[serde(default)]
    pub project: Option<String>,

    /// Path to compose file (relative to .conduit.yml)
    #[serde(default)]
    pub compose_file: Option<String>,

    /// Base domain for auto-generated routes (e.g., "deepiri.localhost")
    #[serde(default)]
    pub domain: Option<String>,

    /// Explicit route overrides per service
    #[serde(default)]
    pub routes: Option<BTreeMap<String, RouteConfig>>,

    /// Service groups for selective startup
    #[serde(default)]
    pub groups: Option<BTreeMap<String, GroupConfig>>,

    /// Services that keep host port bindings (escape hatch)
    #[serde(default)]
    pub expose: Option<BTreeMap<String, u16>>,

    /// Environment variable overrides
    #[serde(default)]
    pub env: Option<BTreeMap<String, String>>,

    /// Health check configuration
    #[serde(default)]
    pub health: Option<HealthConfig>,

    /// Database connection hints for `conduit db`
    #[serde(default)]
    pub databases: Option<BTreeMap<String, DatabaseConfig>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteConfig {
    pub domain: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub websocket: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroupConfig {
    /// Group dependencies (start these groups first)
    #[serde(default)]
    pub depends_on: Option<Vec<String>>,
    /// Services in this group
    pub services: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthConfig {
    /// Max seconds to wait for all services to be healthy
    #[serde(default = "default_health_timeout")]
    pub timeout: u64,
    /// Check interval during startup (seconds)
    #[serde(default = "default_health_interval")]
    pub interval: u64,
}

fn default_health_timeout() -> u64 {
    180
}

fn default_health_interval() -> u64 {
    5
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    /// Database type: postgresql, mongodb, redis, mysql, clickhouse
    #[serde(rename = "type")]
    pub db_type: String,
    /// Env var name for username
    #[serde(default)]
    pub user_env: Option<String>,
    /// Env var name for password
    #[serde(default)]
    pub password_env: Option<String>,
    /// Env var name for database name
    #[serde(default)]
    pub database_env: Option<String>,
    /// Override container port (default: image default, e.g. 5432)
    #[serde(default)]
    pub port: Option<u16>,
}

pub fn load(path: &Path) -> Result<ConduitConfig> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let config: ConduitConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(config)
}

impl ConduitConfig {
    /// Resolve which services to start for a given group name.
    /// Returns all services in the group plus all services in dependent groups (recursively).
    pub fn resolve_group(&self, group_name: &str) -> Vec<String> {
        let groups = match &self.groups {
            Some(g) => g,
            None => return Vec::new(),
        };

        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        self.resolve_group_recursive(group_name, groups, &mut result, &mut visited);
        result
    }

    fn resolve_group_recursive(
        &self,
        name: &str,
        groups: &BTreeMap<String, GroupConfig>,
        result: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if !visited.insert(name.to_string()) {
            return;
        }
        if let Some(group) = groups.get(name) {
            if let Some(deps) = &group.depends_on {
                for dep in deps {
                    self.resolve_group_recursive(dep, groups, result, visited);
                }
            }
            for svc in &group.services {
                if !result.contains(svc) {
                    result.push(svc.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let yaml = r#"
project: deepiri
compose_file: docker-compose.dev.yml
domain: deepiri.localhost
routes:
  frontend-dev:
    domain: frontend.deepiri.localhost
    websocket: true
  api-gateway:
    domain: api.deepiri.localhost
groups:
  infra:
    services: [postgres, redis]
  core:
    depends_on: [infra]
    services: [api-gateway, auth-service]
expose:
  ollama: 11435
"#;
        let config: ConduitConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.project.as_deref(), Some("deepiri"));
        assert_eq!(config.routes.as_ref().unwrap().len(), 2);
        assert_eq!(config.expose.as_ref().unwrap()["ollama"], 11435);
    }

    #[test]
    fn test_resolve_group() {
        let yaml = r#"
groups:
  infra:
    services: [postgres, redis]
  core:
    depends_on: [infra]
    services: [api-gateway, auth-service]
  ai:
    depends_on: [core]
    services: [cyrex, persola]
"#;
        let config: ConduitConfig = serde_yaml::from_str(yaml).unwrap();

        let infra = config.resolve_group("infra");
        assert_eq!(infra, vec!["postgres", "redis"]);

        let core = config.resolve_group("core");
        assert_eq!(
            core,
            vec!["postgres", "redis", "api-gateway", "auth-service"]
        );

        let ai = config.resolve_group("ai");
        assert_eq!(
            ai,
            vec![
                "postgres",
                "redis",
                "api-gateway",
                "auth-service",
                "cyrex",
                "persola"
            ]
        );
    }
}
