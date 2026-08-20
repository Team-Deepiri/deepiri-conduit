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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn cache_dir_nested_path() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            cache_dir(dir.path()),
            dir.path().join(".conduit").join("cache")
        );
    }

    #[test]
    fn write_generated_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let compose = ComposeFile {
            name: Some("demo".into()),
            version: None,
            services: BTreeMap::new(),
            volumes: None,
            networks: None,
        };

        let path = write_generated(dir.path(), &compose).unwrap();
        assert!(path.ends_with("docker-compose.conduit.yml"));

        let yaml = std::fs::read_to_string(&path).unwrap();
        let parsed: ComposeFile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.name.as_deref(), Some("demo"));
        assert!(parsed.services.is_empty());
    }

    #[test]
    fn emitted_yaml_never_contains_null_top_level_fields() {
        // Regression: `docker compose` rejects `version: null` / `name: null`.
        let dir = tempfile::tempdir().unwrap();
        let compose = ComposeFile {
            name: None,
            version: None,
            services: BTreeMap::from([(
                "web".into(),
                super::super::types::Service {
                    image: Some("nginx".into()),
                    ..Default::default()
                },
            )]),
            volumes: None,
            networks: None,
        };

        let path = write_generated(dir.path(), &compose).unwrap();
        let yaml = std::fs::read_to_string(&path).unwrap();
        assert!(
            !yaml.contains("null"),
            "emitted YAML must not contain null:\n{}",
            yaml
        );
        assert!(yaml.contains("web:"));
    }
}
