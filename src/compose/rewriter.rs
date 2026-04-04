use std::collections::BTreeMap;
use tracing::{debug, info};

use super::types::{ComposeFile, Labels, NetworkConfig, ServiceNetworks};
use crate::config::conduit_yml::ConduitConfig;

/// Result of rewriting a compose file.
#[derive(Debug)]
pub struct RewriteResult {
    pub routes: Vec<Route>,
    pub network_name: String,
    pub stripped_ports: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone)]
pub struct Route {
    pub domain: String,
    pub service_name: String,
    pub container_port: u16,
    pub websocket: bool,
}

/// Add only `conduit.*` labels for state tracking — used with `--no-proxy`.
pub fn apply_conduit_labels_only(compose: &mut ComposeFile, project_name: &str) {
    for (svc_name, service) in &mut compose.services {
        let mut labels = service
            .labels
            .as_ref()
            .map(|l| l.as_map())
            .unwrap_or_default();
        labels.insert("conduit.managed".into(), "true".into());
        labels.insert("conduit.project".into(), project_name.into());
        labels.insert("conduit.service".into(), svc_name.clone());
        service.labels = Some(Labels::from_map(labels));
    }
}

/// Rewrite a compose file in-memory for Conduit management:
/// 1. Strip host port bindings (unless in expose config)
/// 2. Inject Traefik routing labels on routable services
/// 3. Replace networks with conduit-managed network
/// 4. Add conduit metadata labels
pub fn rewrite(
    compose: &mut ComposeFile,
    config: &ConduitConfig,
    project_name: &str,
) -> RewriteResult {
    let network_name = format!("conduit-{}", project_name);
    let default_domain = format!("{}.localhost", project_name);
    let domain_base = config.domain.as_deref().unwrap_or(&default_domain);

    let mut routes = Vec::new();
    let mut stripped_ports = Vec::new();

    for (svc_name, service) in &mut compose.services {
        let should_expose = config
            .expose
            .as_ref()
            .map(|e| e.contains_key(svc_name))
            .unwrap_or(false);

        // Must run before stripping `ports`, or `guess_http_port()` returns nothing.
        let guessed_http_port = service.guess_http_port();

        if !should_expose {
            if let Some(ports) = service.ports.take() {
                let port_strs: Vec<String> = ports
                    .iter()
                    .map(|p| match p {
                        super::types::PortMapping::Short(s) => s.clone(),
                        super::types::PortMapping::Long(l) => format!("{}:{}", l.target, l.target),
                    })
                    .collect();
                if !port_strs.is_empty() {
                    debug!("Stripped ports from {}: {:?}", svc_name, port_strs);
                    stripped_ports.push((svc_name.clone(), port_strs));
                }
            }
        }

        let mut labels = service
            .labels
            .as_ref()
            .map(|l| l.as_map())
            .unwrap_or_default();

        labels.insert("conduit.managed".into(), "true".into());
        labels.insert("conduit.project".into(), project_name.into());
        labels.insert("conduit.service".into(), svc_name.clone());

        if let Some(route_cfg) = config.routes.as_ref().and_then(|r| r.get(svc_name)) {
            let domain = &route_cfg.domain;
            let port = route_cfg.port.or(guessed_http_port).unwrap_or(80);
            let router_name = format!("{}-{}", project_name, svc_name);

            labels.insert("traefik.enable".into(), "true".into());
            labels.insert("traefik.docker.network".into(), network_name.clone());
            labels.insert(
                format!("traefik.http.routers.{}.rule", router_name),
                format!("Host(`{}`)", domain),
            );
            labels.insert(
                format!("traefik.http.routers.{}.entrypoints", router_name),
                "web".into(),
            );
            labels.insert(
                format!(
                    "traefik.http.services.{}.loadbalancer.server.port",
                    router_name
                ),
                port.to_string(),
            );

            let ws = route_cfg.websocket.unwrap_or(false);
            if ws {
                labels.insert(
                    format!("traefik.http.routers.{}.middlewares", router_name),
                    format!("{}-ws", router_name),
                );
                labels.insert(
                    format!(
                        "traefik.http.middlewares.{}-ws.headers.customrequestheaders.Connection",
                        router_name
                    ),
                    "keep-alive, Upgrade".into(),
                );
                labels.insert(
                    format!(
                        "traefik.http.middlewares.{}-ws.headers.customrequestheaders.Upgrade",
                        router_name
                    ),
                    "websocket".into(),
                );
            }

            info!("Route: {} → {}:{}", domain, svc_name, port);
            routes.push(Route {
                domain: domain.clone(),
                service_name: svc_name.clone(),
                container_port: port,
                websocket: ws,
            });
        } else if is_http_service(svc_name, service) {
            let domain = format!("{}.{}", svc_name, domain_base);
            if let Some(port) = guessed_http_port {
                let router_name = format!("{}-{}", project_name, svc_name);

                labels.insert("traefik.enable".into(), "true".into());
                labels.insert("traefik.docker.network".into(), network_name.clone());
                labels.insert(
                    format!("traefik.http.routers.{}.rule", router_name),
                    format!("Host(`{}`)", domain),
                );
                labels.insert(
                    format!("traefik.http.routers.{}.entrypoints", router_name),
                    "web".into(),
                );
                labels.insert(
                    format!(
                        "traefik.http.services.{}.loadbalancer.server.port",
                        router_name
                    ),
                    port.to_string(),
                );

                routes.push(Route {
                    domain,
                    service_name: svc_name.clone(),
                    container_port: port,
                    websocket: false,
                });
            }
        }

        service.labels = Some(Labels::from_map(labels));

        service.networks = Some(ServiceNetworks::List(vec![network_name.clone()]));
    }

    compose.networks = Some(BTreeMap::from([(
        network_name.clone(),
        Some(NetworkConfig {
            driver: Some("bridge".into()),
            external: None,
            extra: BTreeMap::new(),
        }),
    )]));

    RewriteResult {
        routes,
        network_name,
        stripped_ports,
    }
}

/// Heuristic: is this service likely an HTTP service that should get a domain?
/// Excludes databases, caches, message brokers, etc.
pub fn is_http_service(name: &str, service: &super::types::Service) -> bool {
    let infra_images = [
        "postgres",
        "mongo",
        "redis",
        "mysql",
        "mariadb",
        "minio",
        "etcd",
        "kafka",
        "zookeeper",
        "elasticsearch",
        "influxdb",
        "milvus",
        "qdrant",
        "weaviate",
        "rabbitmq",
        "nats",
        "memcached",
        "ollama",
    ];

    if let Some(image) = &service.image {
        let image_lower = image.to_lowercase();
        for infra in &infra_images {
            if image_lower.contains(infra) {
                return false;
            }
        }
    }

    let infra_names = [
        "postgres",
        "db",
        "database",
        "redis",
        "cache",
        "mongo",
        "mongodb",
        "mysql",
        "etcd",
        "minio",
        "kafka",
        "zookeeper",
        "elasticsearch",
        "influxdb",
        "milvus",
        "rabbitmq",
        "nats",
        "memcached",
        "ollama",
    ];
    let name_lower = name.to_lowercase();
    for infra in &infra_names {
        if name_lower == *infra {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::types::*;
    use crate::config::conduit_yml::RouteConfig;

    fn make_service(image: &str, port: u16) -> Service {
        Service {
            image: Some(image.to_string()),
            build: None,
            container_name: None,
            ports: Some(vec![PortMapping::Short(format!("{}:{}", port, port))]),
            environment: None,
            env_file: None,
            volumes: None,
            networks: None,
            depends_on: None,
            healthcheck: None,
            labels: None,
            command: None,
            restart: None,
            profiles: None,
            deploy: None,
            logging: None,
            user: None,
            working_dir: None,
            entrypoint: None,
            pull_policy: None,
            runtime: None,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn test_rewrite_strips_ports() {
        let mut compose = ComposeFile {
            name: Some("test".into()),
            version: None,
            services: BTreeMap::from([
                ("web".into(), make_service("nginx", 80)),
                ("db".into(), make_service("postgres:16", 5432)),
            ]),
            volumes: None,
            networks: None,
        };

        let config = ConduitConfig {
            project: Some("test".into()),
            compose_file: None,
            domain: Some("test.localhost".into()),
            routes: None,
            groups: None,
            expose: None,
            env: None,
            health: None,
            databases: None,
        };

        let result = rewrite(&mut compose, &config, "test");

        assert!(compose.services["web"].ports.is_none());
        assert!(compose.services["db"].ports.is_none());
        assert_eq!(result.stripped_ports.len(), 2);
    }

    #[test]
    fn test_rewrite_injects_route_labels() {
        let mut compose = ComposeFile {
            name: Some("myapp".into()),
            version: None,
            services: BTreeMap::from([("api".into(), make_service("node:20", 3000))]),
            volumes: None,
            networks: None,
        };

        let mut routes = BTreeMap::new();
        routes.insert(
            "api".into(),
            RouteConfig {
                domain: "api.myapp.localhost".into(),
                port: Some(3000),
                websocket: None,
            },
        );

        let config = ConduitConfig {
            project: Some("myapp".into()),
            compose_file: None,
            domain: Some("myapp.localhost".into()),
            routes: Some(routes),
            groups: None,
            expose: None,
            env: None,
            health: None,
            databases: None,
        };

        let result = rewrite(&mut compose, &config, "myapp");
        assert_eq!(result.routes.len(), 1);
        assert_eq!(result.routes[0].domain, "api.myapp.localhost");

        let labels = compose.services["api"].labels.as_ref().unwrap().as_map();
        assert_eq!(labels.get("traefik.enable").unwrap(), "true");
        assert!(labels
            .get("traefik.http.routers.myapp-api.rule")
            .unwrap()
            .contains("api.myapp.localhost"));
    }
}
