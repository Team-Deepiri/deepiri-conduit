use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Top-level Docker Compose file representation.
/// Handles Compose v3.x format as output by `docker compose config`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ComposeFile {
    pub name: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub services: BTreeMap<String, Service>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volumes: Option<BTreeMap<String, Option<VolumeConfig>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub networks: Option<BTreeMap<String, Option<NetworkConfig>>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Service {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<PortMapping>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Environment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_file: Option<EnvFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volumes: Option<Vec<serde_yaml::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub networks: Option<ServiceNetworks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<DependsOn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<HealthCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Labels>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy: Option<serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<CommandVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

/// Port mapping: supports both short string syntax and long form.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PortMapping {
    Short(String),
    Long(LongPortMapping),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LongPortMapping {
    pub target: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<PortPublished>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PortPublished {
    Port(u16),
    Range(String),
}

/// Environment variables: either a map or a list of KEY=VALUE strings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Environment {
    Map(BTreeMap<String, Option<String>>),
    List(Vec<String>),
}

impl Environment {
    pub fn as_map(&self) -> BTreeMap<String, String> {
        match self {
            Environment::Map(m) => m
                .iter()
                .filter_map(|(k, v)| v.as_ref().map(|val| (k.clone(), val.clone())))
                .collect(),
            Environment::List(list) => list
                .iter()
                .filter_map(|s| {
                    let mut parts = s.splitn(2, '=');
                    let key = parts.next()?.to_string();
                    let val = parts.next().unwrap_or("").to_string();
                    Some((key, val))
                })
                .collect(),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.as_map().get(key).cloned()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EnvFile {
    Single(String),
    List(Vec<String>),
}

/// Service networks: either a list of names or a map with per-network config.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ServiceNetworks {
    List(Vec<String>),
    Map(BTreeMap<String, Option<ServiceNetworkConfig>>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceNetworkConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ipv4_address: Option<String>,
}

/// depends_on: either a simple list or a map with conditions.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum DependsOn {
    List(Vec<String>),
    Map(BTreeMap<String, DependsOnCondition>),
}

impl DependsOn {
    pub fn service_names(&self) -> Vec<String> {
        match self {
            DependsOn::List(list) => list.clone(),
            DependsOn::Map(map) => map.keys().cloned().collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DependsOnCondition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthCheck {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<HealthCheckTest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_period: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum HealthCheckTest {
    ShellString(String),
    CmdList(Vec<String>),
}

/// Labels: either a map or a list of KEY=VALUE.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Labels {
    Map(BTreeMap<String, String>),
    List(Vec<String>),
}

impl Labels {
    pub fn as_map(&self) -> BTreeMap<String, String> {
        match self {
            Labels::Map(m) => m.clone(),
            Labels::List(list) => list
                .iter()
                .filter_map(|s| {
                    let mut parts = s.splitn(2, '=');
                    let key = parts.next()?.to_string();
                    let val = parts.next().unwrap_or("").to_string();
                    Some((key, val))
                })
                .collect(),
        }
    }

    pub fn set(&mut self, key: String, value: String) {
        match self {
            Labels::Map(m) => {
                m.insert(key, value);
            }
            Labels::List(list) => {
                list.retain(|s| !s.starts_with(&format!("{}=", key)));
                list.push(format!("{}={}", key, value));
            }
        }
    }

    pub fn from_map(map: BTreeMap<String, String>) -> Self {
        Labels::Map(map)
    }
}

impl Default for Labels {
    fn default() -> Self {
        Labels::Map(BTreeMap::new())
    }
}

/// Command: either a string or a list.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CommandVariant {
    String(String),
    List(Vec<String>),
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct BuildConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dockerfile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_yaml::Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct VolumeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_opts: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct NetworkConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

impl PortMapping {
    /// Extract the internal (container) port from a port mapping.
    pub fn internal_port(&self) -> Option<u16> {
        match self {
            PortMapping::Short(s) => {
                // Formats: "8000:8000", "5432:5432/tcp", "127.0.0.1:5432:5432"
                let s = s.split('/').next().unwrap_or(s);
                let parts: Vec<&str> = s.split(':').collect();
                match parts.len() {
                    1 => parts[0].parse().ok(),
                    2 => parts[1].parse().ok(),
                    3 => parts[2].parse().ok(),
                    _ => None,
                }
            }
            PortMapping::Long(l) => Some(l.target),
        }
    }

    /// Extract the published (host) port from a port mapping.
    pub fn host_port(&self) -> Option<u16> {
        match self {
            PortMapping::Short(s) => {
                let s = s.split('/').next().unwrap_or(s);
                let parts: Vec<&str> = s.split(':').collect();
                match parts.len() {
                    1 => None,
                    2 => parts[0].parse().ok(),
                    3 => parts[1].parse().ok(),
                    _ => None,
                }
            }
            PortMapping::Long(l) => l.published.as_ref().and_then(|p| match p {
                PortPublished::Port(port) => Some(*port),
                PortPublished::Range(_) => None,
            }),
        }
    }
}

impl Service {
    /// Best guess at the primary HTTP port this service listens on.
    pub fn guess_http_port(&self) -> Option<u16> {
        if let Some(ports) = &self.ports {
            if let Some(first) = ports.first() {
                return first.internal_port();
            }
        }
        if let Some(env) = &self.environment {
            if let Some(port_str) = env.get("PORT") {
                if let Ok(port) = port_str.parse::<u16>() {
                    return Some(port);
                }
            }
        }
        None
    }
}
