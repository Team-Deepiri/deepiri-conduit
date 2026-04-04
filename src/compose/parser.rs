use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

use super::types::ComposeFile;

/// Parse a Docker Compose file into a typed ComposeFile struct.
///
/// Strategy:
/// 1. Try `docker compose config` to get fully resolved YAML (handles interpolation, extends, etc.)
/// 2. Fall back to raw YAML parsing if docker compose CLI is not available.
pub fn parse(compose_path: &Path) -> Result<ComposeFile> {
    let abs_path = compose_path
        .canonicalize()
        .with_context(|| format!("Compose file not found: {}", compose_path.display()))?;

    match parse_via_docker_cli(&abs_path) {
        Ok(compose) => {
            info!("Parsed compose via `docker compose config`");
            Ok(compose)
        }
        Err(cli_err) => {
            warn!(
                "docker compose config failed ({}), falling back to raw YAML parse",
                cli_err
            );
            parse_raw(&abs_path)
        }
    }
}

fn parse_via_docker_cli(compose_path: &Path) -> Result<ComposeFile> {
    let dir = compose_path
        .parent()
        .context("Cannot determine compose file directory")?;
    let filename = compose_path
        .file_name()
        .context("Cannot determine compose filename")?;

    let output = Command::new("docker")
        .args(["compose", "-f", &filename.to_string_lossy(), "config"])
        .current_dir(dir)
        .output()
        .context("Failed to run `docker compose`. Is Docker installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose config failed: {}", stderr.trim());
    }

    let yaml = String::from_utf8(output.stdout)
        .context("docker compose config output is not valid UTF-8")?;
    debug!("docker compose config output length: {} bytes", yaml.len());

    let compose: ComposeFile =
        serde_yaml::from_str(&yaml).context("Failed to parse docker compose config output")?;

    Ok(compose)
}

fn parse_raw(compose_path: &Path) -> Result<ComposeFile> {
    let contents = std::fs::read_to_string(compose_path).context("Failed to read compose file")?;
    parse_str(&contents)
}

/// Parse compose YAML from memory (tests, fixtures, fallback when Docker is unavailable).
pub fn parse_str(contents: &str) -> Result<ComposeFile> {
    serde_yaml::from_str(contents).context("Failed to parse compose YAML")
}

/// Discover the compose file in a project directory.
/// Checks common filenames in priority order.
pub fn find_compose_file(project_dir: &Path) -> Option<std::path::PathBuf> {
    let candidates = [
        "docker-compose.dev.yml",
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ];

    for candidate in &candidates {
        let path = project_dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_raw_simple() {
        let yaml = r#"
services:
  web:
    image: nginx:alpine
    ports:
      - "8080:80"
  db:
    image: postgres:16
    ports:
      - "5432:5432"
    environment:
      POSTGRES_PASSWORD: secret
"#;
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(yaml.as_bytes()).unwrap();

        let compose = parse_raw(f.path()).unwrap();
        assert_eq!(compose.services.len(), 2);
        assert!(compose.services.contains_key("web"));
        assert!(compose.services.contains_key("db"));

        let web = &compose.services["web"];
        assert_eq!(web.image.as_deref(), Some("nginx:alpine"));

        let db = &compose.services["db"];
        let ports = db.ports.as_ref().unwrap();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].internal_port(), Some(5432));
        assert_eq!(ports[0].host_port(), Some(5432));
    }

    #[test]
    fn test_find_compose_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_compose_file(dir.path()).is_none());

        std::fs::write(dir.path().join("docker-compose.yml"), "services: {}").unwrap();
        let found = find_compose_file(dir.path()).unwrap();
        assert!(found.ends_with("docker-compose.yml"));

        std::fs::write(dir.path().join("docker-compose.dev.yml"), "services: {}").unwrap();
        let found = find_compose_file(dir.path()).unwrap();
        assert!(
            found.ends_with("docker-compose.dev.yml"),
            "dev file should take priority"
        );
    }
}
