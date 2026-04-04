use anyhow::{Context, Result};
use std::process::Command;
use tracing::{debug, info, warn};

const MARKER_START: &str = "# >>> CONDUIT START (do not edit) <<<";
const MARKER_END: &str = "# >>> CONDUIT END <<<";
const HOSTS_PATH: &str = "/etc/hosts";

use crate::registry::state::ConduitState;

/// Reconcile /etc/hosts with all route domains still tracked in state (multi-project safe).
pub fn sync_from_state(state: &ConduitState) -> Result<()> {
    let mut domains: Vec<String> = state
        .projects
        .values()
        .flat_map(|p| p.routes.keys().cloned())
        .collect();
    domains.sort();
    domains.dedup();
    if domains.is_empty() {
        remove_entries()
    } else {
        add_entries(&domains)
    }
}

/// Add domain entries to /etc/hosts between conduit markers.
pub fn add_entries(domains: &[String]) -> Result<()> {
    if domains.is_empty() {
        return Ok(());
    }

    let current = std::fs::read_to_string(HOSTS_PATH)
        .with_context(|| format!("Failed to read {}", HOSTS_PATH))?;

    let cleaned = remove_conduit_block(&current);

    let mut block = String::new();
    block.push_str(MARKER_START);
    block.push('\n');
    for domain in domains {
        block.push_str(&format!("127.0.0.1 {}\n", domain));
    }
    block.push_str(MARKER_END);
    block.push('\n');

    let new_content = format!("{}\n{}", cleaned.trim_end(), block);

    write_hosts(&new_content)?;
    info!("Updated /etc/hosts with {} domain entries", domains.len());

    if is_wsl() {
        sync_wsl_hosts(domains);
    }

    Ok(())
}

/// Remove all conduit entries from /etc/hosts.
pub fn remove_entries() -> Result<()> {
    let current = match std::fs::read_to_string(HOSTS_PATH) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let cleaned = remove_conduit_block(&current);
    if cleaned != current {
        write_hosts(&cleaned)?;
        info!("Removed conduit entries from /etc/hosts");
    }

    Ok(())
}

/// List domains currently in the conduit block.
pub fn current_entries() -> Result<Vec<String>> {
    let current = std::fs::read_to_string(HOSTS_PATH).unwrap_or_default();
    let mut in_block = false;
    let mut domains = Vec::new();

    for line in current.lines() {
        if line.trim() == MARKER_START {
            in_block = true;
            continue;
        }
        if line.trim() == MARKER_END {
            in_block = false;
            continue;
        }
        if in_block {
            if let Some(domain) = line.strip_prefix("127.0.0.1 ") {
                domains.push(domain.trim().to_string());
            }
        }
    }

    Ok(domains)
}

fn remove_conduit_block(content: &str) -> String {
    let mut result = String::new();
    let mut in_block = false;

    for line in content.lines() {
        if line.trim() == MARKER_START {
            in_block = true;
            continue;
        }
        if line.trim() == MARKER_END {
            in_block = false;
            continue;
        }
        if !in_block {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

fn write_hosts(content: &str) -> Result<()> {
    match std::fs::write(HOSTS_PATH, content) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            debug!("Direct write to /etc/hosts failed, trying sudo");
            write_hosts_sudo(content)
        }
        Err(e) => Err(e).context("Failed to write /etc/hosts"),
    }
}

fn write_hosts_sudo(content: &str) -> Result<()> {
    let output = Command::new("sudo")
        .args(["tee", HOSTS_PATH])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(content.as_bytes())?;
            }
            child.wait_with_output()
        })
        .context("Failed to run sudo tee for /etc/hosts")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "sudo tee /etc/hosts failed: {}. Run conduit with sudo or use --no-proxy.",
            stderr.trim()
        );
    }

    Ok(())
}

fn is_wsl() -> bool {
    std::fs::read_to_string("/proc/version")
        .map(|v| v.to_lowercase().contains("microsoft"))
        .unwrap_or(false)
}

fn sync_wsl_hosts(domains: &[String]) {
    let entries: Vec<String> = domains.iter().map(|d| format!("127.0.0.1 {}", d)).collect();
    let content = entries.join("\n");

    let script = format!(
        r#"$hostsPath = 'C:\Windows\System32\drivers\etc\hosts'
$content = Get-Content $hostsPath -Raw
$startMarker = '# >>> CONDUIT START (do not edit) <<<'
$endMarker = '# >>> CONDUIT END <<<'
$pattern = "(?s)$([regex]::Escape($startMarker)).*?$([regex]::Escape($endMarker))\r?\n?"
$content = $content -replace $pattern, ''
$block = "$startMarker`r`n{}`r`n$endMarker`r`n"
$content = $content.TrimEnd() + "`r`n" + $block
Set-Content -Path $hostsPath -Value $content -Force"#,
        content.replace('\n', "`r`n")
    );

    match Command::new("powershell.exe")
        .args(["-Command", &script])
        .output()
    {
        Ok(output) if output.status.success() => {
            info!("Synced {} domains to Windows hosts file", domains.len());
        }
        Ok(output) => {
            warn!(
                "Failed to sync Windows hosts file: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(e) => {
            warn!("Failed to run powershell.exe for hosts sync: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_conduit_block() {
        let input = "127.0.0.1 localhost\n\
                      # >>> CONDUIT START (do not edit) <<<\n\
                      127.0.0.1 app.test.localhost\n\
                      127.0.0.1 api.test.localhost\n\
                      # >>> CONDUIT END <<<\n\
                      ::1 localhost\n";

        let result = remove_conduit_block(input);
        assert!(!result.contains("CONDUIT"));
        assert!(!result.contains("app.test.localhost"));
        assert!(result.contains("127.0.0.1 localhost"));
        assert!(result.contains("::1 localhost"));
    }

    #[test]
    fn test_remove_conduit_block_no_block() {
        let input = "127.0.0.1 localhost\n::1 localhost\n";
        let result = remove_conduit_block(input);
        assert_eq!(result, input);
    }
}
