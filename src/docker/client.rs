use anyhow::{Context, Result};
use bollard::Docker;
use tracing::debug;

/// Create a Docker client connected to the local Docker daemon.
pub async fn connect() -> Result<Docker> {
    let docker = Docker::connect_with_defaults()
        .context("Failed to connect to Docker. Is Docker running?")?;

    let version = docker.version().await.context(
        "Connected to Docker socket but failed to get version. Is Docker daemon healthy?",
    )?;

    debug!(
        "Docker version: {} (API: {})",
        version.version.as_deref().unwrap_or("unknown"),
        version.api_version.as_deref().unwrap_or("unknown")
    );

    Ok(docker)
}

/// Check Docker is running and return version info.
pub async fn check_docker() -> Result<DockerInfo> {
    let docker = connect().await?;
    let version = docker.version().await?;
    let info = docker.info().await?;

    Ok(DockerInfo {
        version: version.version.unwrap_or_default(),
        api_version: version.api_version.unwrap_or_default(),
        os: version.os.unwrap_or_default(),
        arch: version.arch.unwrap_or_default(),
        containers_running: info.containers_running.unwrap_or(0) as u32,
    })
}

#[derive(Debug)]
pub struct DockerInfo {
    pub version: String,
    pub api_version: String,
    pub os: String,
    pub arch: String,
    pub containers_running: u32,
}
