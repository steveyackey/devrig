pub mod interpolate;
pub mod model;
pub mod resolve;
pub mod validate;

use std::path::Path;

use model::DevrigConfig;

pub fn load_config(path: &Path) -> anyhow::Result<DevrigConfig> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", path.display(), e))?;
    let config: DevrigConfig = toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file {}: {}", path.display(), e))?;
    Ok(config)
}
