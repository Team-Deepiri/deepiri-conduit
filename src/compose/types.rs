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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_port_internal_port() {
        assert_eq!(
            PortMapping::Short("8000:8000".into()).internal_port(),
            Some(8000)
        );
        assert_eq!(
            PortMapping::Short("5432:5432/tcp".into()).internal_port(),
            Some(5432)
        );
        assert_eq!(
            PortMapping::Short("127.0.0.1:5432:5432".into()).internal_port(),
            Some(5432)
        );
        assert_eq!(
            PortMapping::Short("8080".into()).internal_port(),
            Some(8080)
        );
        assert_eq!(
            PortMapping::Short("not-a-port".into()).internal_port(),
            None
        );
    }

    #[test]
    fn short_port_host_port() {
        assert_eq!(
            PortMapping::Short("8000:8000".into()).host_port(),
            Some(8000)
        );
        assert_eq!(
            PortMapping::Short("127.0.0.1:5432:5432".into()).host_port(),
            Some(5432)
        );
        assert_eq!(PortMapping::Short("8080".into()).host_port(), None);
    }

    #[test]
    fn long_port_internal_and_host() {
        let mapped = PortMapping::Long(LongPortMapping {
            target: 5432,
            published: Some(PortPublished::Port(15432)),
            host_ip: None,
            protocol: Some("tcp".into()),
        });
        assert_eq!(mapped.internal_port(), Some(5432));
        assert_eq!(mapped.host_port(), Some(15432));

        let ranged = PortMapping::Long(LongPortMapping {
            target: 5432,
            published: Some(PortPublished::Range("15432-15440".into())),
            host_ip: None,
            protocol: None,
        });
        assert_eq!(ranged.host_port(), None);
    }

    #[test]
    fn environment_map_and_list() {
        let map = Environment::Map(BTreeMap::from([
            ("FOO".into(), Some("bar".into())),
            ("EMPTY".into(), None),
        ]));
        assert_eq!(map.get("FOO").as_deref(), Some("bar"));
        assert_eq!(map.get("EMPTY"), None);

        let list = Environment::List(vec!["A=1".into(), "B".into()]);
        assert_eq!(list.get("A").as_deref(), Some("1"));
        assert_eq!(list.get("B").as_deref(), Some(""));
    }

    #[test]
    fn labels_map_and_list_operations() {
        let mut map_labels = Labels::Map(BTreeMap::from([("k".into(), "v".into())]));
        map_labels.set("k2".into(), "v2".into());
        assert_eq!(
            map_labels.as_map().get("k2").map(String::as_str),
            Some("v2")
        );

        let mut list_labels = Labels::List(vec!["a=1".into(), "b=2".into()]);
        list_labels.set("a".into(), "3".into());
        let list_map = list_labels.as_map();
        assert_eq!(list_map.get("a").map(String::as_str), Some("3"));
        assert_eq!(list_map.len(), 2);
    }

    #[test]
    fn depends_on_service_names() {
        let list = DependsOn::List(vec!["db".into(), "redis".into()]);
        assert_eq!(
            list.service_names(),
            vec!["db".to_string(), "redis".to_string()]
        );

        let map = DependsOn::Map(BTreeMap::from([(
            "db".into(),
            DependsOnCondition {
                condition: None,
                restart: None,
            },
        )]));
        assert_eq!(map.service_names(), vec!["db".to_string()]);
    }

    #[test]
    fn guess_http_port_prefers_ports_over_env() {
        let svc: Service = serde_yaml::from_str(
            "image: nginx\nports:\n  - \"3000:3000\"\nenvironment:\n  PORT: \"8080\"",
        )
        .unwrap();
        assert_eq!(svc.guess_http_port(), Some(3000));
    }

    #[test]
    fn guess_http_port_falls_back_to_port_env() {
        let svc: Service =
            serde_yaml::from_str("image: nginx\nenvironment:\n  PORT: \"8080\"").unwrap();
        assert_eq!(svc.guess_http_port(), Some(8080));
    }

    #[test]
    fn guess_http_port_returns_none_when_unknown() {
        let svc: Service = serde_yaml::from_str("image: nginx").unwrap();
        assert_eq!(svc.guess_http_port(), None);
    }

    #[test]
    fn compose_file_serde_roundtrip() {
        let yaml = r#"
name: demo
services:
  web:
    image: nginx
    ports:
      - "80:80"
    volumes:
      - ./html:/usr/share/nginx/html
  db:
    image: postgres:16
volumes:
  data: {}
networks:
  backend:
    driver: bridge
"#;
        let compose: ComposeFile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(compose.name.as_deref(), Some("demo"));
        assert_eq!(compose.services.len(), 2);
        assert!(compose.volumes.as_ref().unwrap().contains_key("data"));
        assert!(compose.networks.as_ref().unwrap().contains_key("backend"));

        let re_serialized = serde_yaml::to_string(&compose).unwrap();
        let parsed_again: ComposeFile = serde_yaml::from_str(&re_serialized).unwrap();
        assert_eq!(parsed_again.services.len(), compose.services.len());
    }

    #[test]
    fn service_keeps_unknown_fields_in_extra() {
        let svc: Service = serde_yaml::from_str("image: nginx\ncap_add:\n  - NET_ADMIN").unwrap();
        assert!(svc.extra.contains_key("cap_add"));
    }
}
