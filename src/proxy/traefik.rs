use crate::config::global::GlobalConfig;

/// Static Traefik config: Docker provider reads routing labels from containers.
/// Written to `~/.local/share/conduit/proxy/traefik.yml` and bind-mounted into the proxy.
pub fn static_config(global_config: &GlobalConfig) -> String {
    let proxy = &global_config.proxy;
    let dashboard = if proxy.dashboard {
        "\napi:\n  dashboard: true\n  insecure: true\n".to_string()
    } else {
        String::new()
    };

    format!(
        r#"entryPoints:
  web:
    address: ":{http_port}"

providers:
  docker:
    endpoint: "unix:///var/run/docker.sock"
    exposedByDefault: false
    watch: true

{dashboard}
log:
  level: INFO
"#,
        http_port = proxy.http_port,
        dashboard = dashboard,
    )
}

/// Path where static config is stored (host).
pub fn host_config_path() -> std::path::PathBuf {
    crate::config::global::state_dir()
        .join("proxy")
        .join("traefik.yml")
}
