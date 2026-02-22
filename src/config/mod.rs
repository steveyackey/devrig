pub mod diff;
pub mod interpolate;
pub mod model;
pub mod resolve;
pub mod validate;
pub mod watcher;

use std::path::Path;

use model::DevrigConfig;

/// Load and parse a devrig config file, returning both the parsed config and
/// the raw TOML source text (needed for validation diagnostics with source spans).
pub fn load_config(path: &Path) -> anyhow::Result<(DevrigConfig, String)> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", path.display(), e))?;
    let config: DevrigConfig = toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file {}: {}", path.display(), e))?;
    Ok((config, content))
}
