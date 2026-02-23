pub mod diff;
pub mod interpolate;
pub mod model;
pub mod resolve;
pub mod secrets;
pub mod validate;
pub mod watcher;

use std::path::Path;

use model::DevrigConfig;
use secrets::SecretRegistry;

/// Load and parse a devrig config file, returning both the parsed config and
/// the raw TOML source text (needed for validation diagnostics with source spans).
pub fn load_config(path: &Path) -> anyhow::Result<(DevrigConfig, String)> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", path.display(), e))?;
    let mut config: DevrigConfig = toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file {}: {}", path.display(), e))?;

    // Auto-discover compose services when services list is empty
    discover_compose_services(&mut config, path);

    Ok((config, content))
}

/// If a `[compose]` section exists with an empty `services` list, parse the
/// docker-compose file to discover available service names. This lets compose
/// services work as `depends_on` targets without explicit enumeration.
fn discover_compose_services(config: &mut DevrigConfig, config_path: &Path) {
    if let Some(compose) = &mut config.compose {
        if compose.services.is_empty() {
            let config_dir = config_path.parent().unwrap_or(Path::new("."));
            let compose_file = config_dir.join(&compose.file);
            compose.services =
                crate::compose::lifecycle::discover_compose_services(&compose_file);
        }
    }
}

/// Load a config file with full secrets processing: .env file loading,
/// $VAR expansion, and secret tracking for masking.
///
/// Pipeline: Parse TOML → Load .env files → Merge .env values → Expand $VAR → Return
pub fn load_config_with_secrets(
    path: &Path,
) -> anyhow::Result<(DevrigConfig, String, SecretRegistry)> {
    let (mut config, source) = load_config(path)?;
    let config_dir = path.parent().unwrap_or(Path::new("."));

    // Load .env files into a lookup pool (for $VAR expansion)
    let env_file_vars = secrets::load_env_files(&config, config_dir)?;

    // Merge .env file values into config.env / service.env (lower priority than TOML)
    secrets::merge_env_file_values(&mut config, config_dir)?;

    // Expand $VAR across all config string fields, tracking secrets
    let registry = secrets::expand_config_env_vars(&mut config, &env_file_vars)?;

    Ok((config, source, registry))
}
