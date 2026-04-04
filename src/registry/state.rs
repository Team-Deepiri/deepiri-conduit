use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::global;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConduitState {
    pub version: u32,
    #[serde(default)]
    pub proxy: Option<ProxyState>,
    #[serde(default)]
    pub projects: BTreeMap<String, ProjectState>,
    #[serde(default)]
    pub tunnels: BTreeMap<String, TunnelState>,
    #[serde(default)]
    pub hosts_entries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyState {
    pub container_id: String,
    pub image: String,
    pub status: String,
    pub http_port: u16,
    pub https_port: u16,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    pub directory: String,
    /// Original compose filename (e.g. docker-compose.dev.yml)
    pub compose_file: String,
    /// Generated compose used for `docker compose` (under .conduit/cache/)
    #[serde(default = "default_generated_compose")]
    pub generated_compose: String,
    /// Value passed to `docker compose -p`
    #[serde(default)]
    pub compose_project_name: String,
    pub network: String,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub services: BTreeMap<String, ServiceState>,
    #[serde(default)]
    pub routes: BTreeMap<String, String>,
}

fn default_generated_compose() -> String {
    crate::compose::emit::GENERATED_REL_PATH.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    pub container_id: String,
    pub container_name: String,
    pub image: String,
    pub status: String,
    #[serde(default)]
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelState {
    pub host_port: u16,
    pub container_name: String,
    pub container_port: u16,
    pub pid: u32,
    pub opened_at: DateTime<Utc>,
}

fn state_file_path() -> PathBuf {
    global::state_dir().join("state.json")
}

/// Load state from disk.
pub fn load() -> Result<ConduitState> {
    let path = state_file_path();
    if !path.exists() {
        return Ok(ConduitState {
            version: 1,
            ..Default::default()
        });
    }

    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read state file {}", path.display()))?;

    let state: ConduitState = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse state file {}", path.display()))?;

    Ok(state)
}

/// Save state to disk (with file locking).
pub fn save(state: &ConduitState) -> Result<()> {
    use fs2::FileExt;
    use std::io::Write;

    let path = state_file_path();
    let dir = path.parent().unwrap();

    std::fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create state directory {}", dir.display()))?;

    let json = serde_json::to_string_pretty(state).context("Failed to serialize state")?;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .with_context(|| format!("Failed to open state file {}", path.display()))?;

    file.lock_exclusive().context("Failed to lock state file")?;
    file.write_all(json.as_bytes())
        .with_context(|| format!("Failed to write state file {}", path.display()))?;
    file.sync_all().ok();

    Ok(())
}

/// Remove a project from state.
pub fn remove_project(project_name: &str) -> Result<()> {
    let mut state = load()?;
    state.projects.remove(project_name);
    save(&state)
}

/// Get project state if it exists.
pub fn get_project(project_name: &str) -> Result<Option<ProjectState>> {
    let state = load()?;
    Ok(state.projects.get(project_name).cloned())
}

/// Check if a project is tracked in state.
pub fn project_exists(project_name: &str) -> Result<bool> {
    let state = load()?;
    Ok(state.projects.contains_key(project_name))
}

/// List all tracked project names.
pub fn list_projects() -> Result<Vec<String>> {
    let state = load()?;
    Ok(state.projects.keys().cloned().collect())
}
