use anyhow::Result;
use bollard::container::{ListContainersOptions, RemoveContainerOptions, StopContainerOptions};
use bollard::models::HealthStatusEnum;
use bollard::Docker;
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// List all containers managed by Conduit for a specific project.
pub async fn list_project_containers(
    docker: &Docker,
    project_name: &str,
) -> Result<Vec<ContainerInfo>> {
    let project_label = format!("conduit.project={}", project_name);
    let mut filters: HashMap<String, Vec<String>> = HashMap::new();
    filters.insert(
        "label".to_string(),
        vec!["conduit.managed=true".to_string(), project_label],
    );

    let options = ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };

    let containers = docker.list_containers(Some(options)).await?;
    let mut result = Vec::new();

    for c in containers {
        let labels = c.labels.unwrap_or_default();
        if labels.get("conduit.project").map(|s| s.as_str()) != Some(project_name) {
            continue;
        }
        result.push(ContainerInfo {
            id: c.id.unwrap_or_default(),
            name: c
                .names
                .and_then(|n| n.first().cloned())
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string(),
            service: labels.get("conduit.service").cloned().unwrap_or_default(),
            image: c.image.unwrap_or_default(),
            state: c.state.unwrap_or_default(),
            status: c.status.unwrap_or_default(),
        });
    }

    Ok(result)
}

/// List ALL conduit-managed containers across all projects.
pub async fn list_all_conduit_containers(docker: &Docker) -> Result<Vec<ContainerInfo>> {
    let mut filters = HashMap::new();
    filters.insert("label", vec!["conduit.managed=true"]);

    let options = ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };

    let containers = docker.list_containers(Some(options)).await?;
    let mut result = Vec::new();

    for c in containers {
        let labels = c.labels.unwrap_or_default();
        result.push(ContainerInfo {
            id: c.id.unwrap_or_default(),
            name: c
                .names
                .and_then(|n| n.first().cloned())
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string(),
            service: labels.get("conduit.service").cloned().unwrap_or_default(),
            image: c.image.unwrap_or_default(),
            state: c.state.unwrap_or_default(),
            status: c.status.unwrap_or_default(),
        });
    }

    Ok(result)
}

/// Stop all containers for a project.
pub async fn stop_project_containers(docker: &Docker, project_name: &str) -> Result<u32> {
    let containers = list_project_containers(docker, project_name).await?;
    let mut stopped = 0u32;

    for c in &containers {
        if c.state == "running" {
            match docker
                .stop_container(&c.id, Some(StopContainerOptions { t: 10 }))
                .await
            {
                Ok(_) => {
                    debug!("Stopped container {} ({})", c.name, c.service);
                    stopped += 1;
                }
                Err(e) => {
                    warn!("Failed to stop container {}: {}", c.name, e);
                }
            }
        }
    }

    info!(
        "Stopped {} containers for project {}",
        stopped, project_name
    );
    Ok(stopped)
}

/// Remove all containers for a project.
pub async fn remove_project_containers(docker: &Docker, project_name: &str) -> Result<u32> {
    let containers = list_project_containers(docker, project_name).await?;
    let mut removed = 0u32;

    for c in &containers {
        match docker
            .remove_container(
                &c.id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
        {
            Ok(_) => {
                debug!("Removed container {} ({})", c.name, c.service);
                removed += 1;
            }
            Err(e) => {
                warn!("Failed to remove container {}: {}", c.name, e);
            }
        }
    }

    info!(
        "Removed {} containers for project {}",
        removed, project_name
    );
    Ok(removed)
}

/// Environment variables from container config (`KEY=value`).
pub async fn container_env_map(
    docker: &Docker,
    container_id: &str,
) -> Result<HashMap<String, String>> {
    let info = docker.inspect_container(container_id, None).await?;
    let mut map = HashMap::new();
    if let Some(cfg) = info.config {
        if let Some(env) = cfg.env {
            for line in env {
                if let Some((k, v)) = line.split_once('=') {
                    map.insert(k.to_string(), v.to_string());
                } else {
                    map.insert(line, String::new());
                }
            }
        }
    }
    Ok(map)
}

/// Get health status of a container.
pub async fn get_health_status(docker: &Docker, container_id: &str) -> Result<HealthStatus> {
    let info = docker.inspect_container(container_id, None).await?;
    let state = info.state.unwrap_or_default();

    if let Some(health) = &state.health {
        match health.status {
            Some(HealthStatusEnum::HEALTHY) => return Ok(HealthStatus::Healthy),
            Some(HealthStatusEnum::UNHEALTHY) => return Ok(HealthStatus::Unhealthy),
            Some(HealthStatusEnum::STARTING) => return Ok(HealthStatus::Starting),
            Some(HealthStatusEnum::NONE) | Some(HealthStatusEnum::EMPTY) | None => {}
        }
    }

    if state.running.unwrap_or(false) {
        Ok(HealthStatus::Running)
    } else {
        Ok(HealthStatus::Stopped)
    }
}

#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub service: String,
    pub image: String,
    pub state: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Starting,
    Running,
    Stopped,
    None,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
            HealthStatus::Starting => write!(f, "starting"),
            HealthStatus::Running => write!(f, "running"),
            HealthStatus::Stopped => write!(f, "stopped"),
            HealthStatus::None => write!(f, "—"),
        }
    }
}
