use anyhow::{Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};
use bollard::models::{HostConfig, PortBinding};
use bollard::Docker;
use futures_util::StreamExt;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::config::global::GlobalConfig;

const PROXY_CONTAINER_NAME: &str = "conduit-proxy";
pub const PROXY_NETWORK: &str = "conduit-proxy-net";

/// Ensure the conduit proxy (Traefik + Docker provider) is running.
pub async fn ensure_running(docker: &Docker, global_config: &GlobalConfig) -> Result<()> {
    let proxy_cfg = &global_config.proxy;

    crate::docker::network::create_network(docker, PROXY_NETWORK).await?;

    write_static_config_file(global_config)?;

    if let Some(status) = get_proxy_status(docker).await? {
        if status == "running" {
            debug!("Proxy already running");
            return Ok(());
        }
        info!("Proxy container exists but is {}, starting", status);
        docker
            .start_container(PROXY_CONTAINER_NAME, None::<StartContainerOptions<String>>)
            .await
            .context("Failed to start existing proxy container")?;
        crate::docker::network::connect_container(docker, PROXY_NETWORK, PROXY_CONTAINER_NAME)
            .await
            .ok();
        return Ok(());
    }

    pull_image_if_needed(docker, &proxy_cfg.image).await?;

    let config_path = super::traefik::host_config_path();
    let config_path_str = config_path
        .to_str()
        .context("Invalid UTF-8 in proxy config path")?;

    let socket_path = docker_socket_path();

    let mut port_bindings = HashMap::new();
    port_bindings.insert(
        format!("{}/tcp", proxy_cfg.http_port),
        Some(vec![PortBinding {
            host_ip: Some("0.0.0.0".into()),
            host_port: Some(proxy_cfg.http_port.to_string()),
        }]),
    );

    if proxy_cfg.dashboard {
        port_bindings.insert(
            "8080/tcp".to_string(),
            Some(vec![PortBinding {
                host_ip: Some("127.0.0.1".into()),
                host_port: Some(proxy_cfg.dashboard_port.to_string()),
            }]),
        );
    }

    let binds = vec![
        format!("{}:/var/run/docker.sock:ro", socket_path),
        format!("{}:/etc/traefik/traefik.yml:ro", config_path_str),
    ];

    let container_config = Config {
        image: Some(proxy_cfg.image.clone()),
        // Official image: entrypoint runs `traefik`; pass flags only (see `docker run traefik --help`).
        cmd: Some(vec!["--configFile=/etc/traefik/traefik.yml".to_string()]),
        labels: Some(HashMap::from([(
            "conduit.proxy".to_string(),
            "true".to_string(),
        )])),
        host_config: Some(HostConfig {
            port_bindings: Some(port_bindings),
            binds: Some(binds),
            restart_policy: Some(bollard::models::RestartPolicy {
                name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    docker
        .create_container(
            Some(CreateContainerOptions {
                name: PROXY_CONTAINER_NAME,
                ..Default::default()
            }),
            container_config,
        )
        .await
        .context("Failed to create proxy container")?;

    docker
        .start_container(PROXY_CONTAINER_NAME, None::<StartContainerOptions<String>>)
        .await
        .context("Failed to start proxy container")?;

    crate::docker::network::connect_container(docker, PROXY_NETWORK, PROXY_CONTAINER_NAME).await?;

    info!(
        "Proxy started ({}) — HTTP :{} (Docker provider)",
        proxy_cfg.image, proxy_cfg.http_port
    );

    Ok(())
}

fn docker_socket_path() -> String {
    std::env::var("DOCKER_HOST")
        .ok()
        .and_then(|h| {
            h.strip_prefix("unix://")
                .map(|s| s.to_string())
                .or_else(|| {
                    if h.starts_with("unix:") {
                        Some(h.trim_start_matches("unix:").to_string())
                    } else {
                        None
                    }
                })
        })
        .unwrap_or_else(|| "/var/run/docker.sock".to_string())
}

fn write_static_config_file(global_config: &GlobalConfig) -> Result<()> {
    let path = super::traefik::host_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let yaml = super::traefik::static_config(global_config);
    std::fs::write(&path, yaml).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Connect the proxy to a project's network so it can route to project containers.
pub async fn connect_to_project_network(docker: &Docker, project_network: &str) -> Result<()> {
    crate::docker::network::connect_container(docker, project_network, PROXY_CONTAINER_NAME).await
}

/// Disconnect the proxy from a project's network.
pub async fn disconnect_from_project_network(docker: &Docker, project_network: &str) -> Result<()> {
    crate::docker::network::disconnect_container(docker, project_network, PROXY_CONTAINER_NAME)
        .await
}

/// Stop and remove the proxy container.
pub async fn stop(docker: &Docker) -> Result<()> {
    match docker
        .stop_container(PROXY_CONTAINER_NAME, Some(StopContainerOptions { t: 5 }))
        .await
    {
        Ok(_) => info!("Stopped proxy"),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {
            debug!("Proxy not found");
            return Ok(());
        }
        Err(e) => warn!("Failed to stop proxy: {}", e),
    }

    let _ = docker
        .remove_container(
            PROXY_CONTAINER_NAME,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await;

    Ok(())
}

/// Get the current status of the proxy container.
pub async fn get_proxy_status(docker: &Docker) -> Result<Option<String>> {
    let mut filters = HashMap::new();
    filters.insert("name", vec![PROXY_CONTAINER_NAME]);

    let options = ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };

    let containers = docker.list_containers(Some(options)).await?;
    for c in containers {
        let names = c.names.unwrap_or_default();
        if names
            .iter()
            .any(|n| n.trim_start_matches('/') == PROXY_CONTAINER_NAME)
        {
            return Ok(Some(c.state.unwrap_or_default()));
        }
    }

    Ok(None)
}

pub fn proxy_network_name() -> &'static str {
    PROXY_NETWORK
}

async fn pull_image_if_needed(docker: &Docker, image: &str) -> Result<()> {
    use bollard::image::CreateImageOptions;

    let parts: Vec<&str> = image.splitn(2, ':').collect();
    let (from_image, tag) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        (image, "latest")
    };

    info!("Pulling {} (if needed)...", image);
    let mut stream = docker.create_image(
        Some(CreateImageOptions {
            from_image,
            tag,
            ..Default::default()
        }),
        None,
        None,
    );

    while let Some(result) = stream.next().await {
        match result {
            Ok(info) => {
                if let Some(status) = info.status {
                    debug!("Pull: {}", status);
                }
            }
            Err(e) => {
                warn!("Pull warning: {}", e);
            }
        }
    }

    Ok(())
}
