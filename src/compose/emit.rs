use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::types::ComposeFile;

pub const GENERATED_REL_PATH: &str = ".conduit/cache/docker-compose.conduit.yml";

pub fn cache_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".conduit").join("cache")
}

/// Write the rewritten compose file used by `docker compose -f`.
pub fn write_generated(project_dir: &Path, compose: &ComposeFile) -> Result<PathBuf> {
    let dir = cache_dir(project_dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("mkdir {}", dir.display()))?;
    let path = dir.join("docker-compose.conduit.yml");
    let yaml = serde_yaml::to_string(compose).context("serialize compose to YAML")?;
    std::fs::write(&path, yaml).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}
