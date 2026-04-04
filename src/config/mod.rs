pub mod conduit_yml;
pub mod global;

use anyhow::Result;
use std::path::Path;

pub use conduit_yml::ConduitConfig;
pub use global::GlobalConfig;

/// Load project config from .conduit.yml if it exists, otherwise return defaults.
pub fn load_project_config(project_dir: &Path) -> Result<ConduitConfig> {
    let config_path = project_dir.join(".conduit.yml");
    if config_path.exists() {
        conduit_yml::load(&config_path)
    } else {
        Ok(ConduitConfig::default())
    }
}

/// Load global config from ~/.config/conduit/config.toml if it exists.
pub fn load_global_config() -> Result<GlobalConfig> {
    global::load()
}
