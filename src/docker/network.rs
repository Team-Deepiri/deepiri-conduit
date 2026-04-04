use anyhow::{Context, Result};
use bollard::models::EndpointSettings;
use bollard::network::{ConnectNetworkOptions, CreateNetworkOptions, DisconnectNetworkOptions};
use bollard::Docker;
use tracing::{debug, info, warn};

/// Create a Docker network for a Conduit project.
pub async fn create_network(docker: &Docker, name: &str) -> Result<String> {
    let existing = docker.list_networks::<String>(None).await?;
    for net in &existing {
        if net.name.as_deref() == Some(name) {
            info!("Network {} already exists, reusing", name);
            return Ok(net.id.clone().unwrap_or_default());
        }
    }

    let options = CreateNetworkOptions {
        name: name.to_string(),
        driver: "bridge".to_string(),
        labels: std::collections::HashMap::from([(
            "conduit.managed".to_string(),
            "true".to_string(),
        )]),
        ..Default::default()
    };

    let response = docker
        .create_network(options)
        .await
        .with_context(|| format!("Failed to create network {}", name))?;

    let id = response.id;
    info!("Created network {} ({})", name, &id[..12.min(id.len())]);
    Ok(id)
}

/// Remove a Docker network.
pub async fn remove_network(docker: &Docker, name: &str) -> Result<()> {
    match docker.remove_network(name).await {
        Ok(_) => {
            info!("Removed network {}", name);
            Ok(())
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {
            debug!("Network {} not found, already removed", name);
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("Failed to remove network {}", name)),
    }
}

/// Connect a container to a network.
pub async fn connect_container(docker: &Docker, network: &str, container: &str) -> Result<()> {
    let config = ConnectNetworkOptions {
        container: container.to_string(),
        endpoint_config: EndpointSettings::default(),
    };

    docker
        .connect_network(network, config)
        .await
        .with_context(|| {
            format!(
                "Failed to connect container {} to network {}",
                container, network
            )
        })?;

    debug!("Connected {} to network {}", container, network);
    Ok(())
}

/// Disconnect a container from a network.
pub async fn disconnect_container(docker: &Docker, network: &str, container: &str) -> Result<()> {
    let config = DisconnectNetworkOptions {
        container: container.to_string(),
        force: true,
    };

    match docker.disconnect_network(network, config).await {
        Ok(_) => {
            debug!("Disconnected {} from network {}", container, network);
            Ok(())
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {
            warn!(
                "Container {} or network {} not found during disconnect",
                container, network
            );
            Ok(())
        }
        Err(e) => Err(e).with_context(|| {
            format!(
                "Failed to disconnect container {} from network {}",
                container, network
            )
        }),
    }
}

/// List all conduit-managed networks.
pub async fn list_conduit_networks(docker: &Docker) -> Result<Vec<String>> {
    let networks = docker.list_networks::<String>(None).await?;
    let conduit_nets: Vec<String> = networks
        .iter()
        .filter(|n| {
            n.labels
                .as_ref()
                .map(|l| l.get("conduit.managed") == Some(&"true".to_string()))
                .unwrap_or(false)
        })
        .filter_map(|n| n.name.clone())
        .collect();
    Ok(conduit_nets)
}

/// Get the IP address of a container on a specific network.
/// If `network_name` is empty or the container has no IP on that network, uses the first
/// attached network with a non-empty IPv4 (e.g. `conduit up --no-proxy` without a conduit network).
pub async fn get_container_ip(
    docker: &Docker,
    container_id: &str,
    network_name: &str,
) -> Result<String> {
    let info = docker.inspect_container(container_id, None).await?;
    let networks = info.network_settings.and_then(|ns| ns.networks);

    if !network_name.is_empty() {
        if let Some(ref nets) = networks {
            if let Some(net) = nets.get(network_name) {
                if let Some(ip) = net.ip_address.as_ref().filter(|s| !s.is_empty()) {
                    return Ok(ip.clone());
                }
            }
        }
    }

    if let Some(nets) = networks {
        let mut pairs: Vec<_> = nets.iter().collect();
        pairs.sort_by_key(|(name, _)| (*name).clone());
        for (_name, net) in pairs {
            if let Some(ip) = net.ip_address.as_ref().filter(|s| !s.is_empty()) {
                return Ok(ip.clone());
            }
        }
    }

    anyhow::bail!(
        "No usable IP for container {} (network {:?})",
        container_id,
        network_name
    )
}
