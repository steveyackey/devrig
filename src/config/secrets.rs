use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{bail, Context, Result};
use regex::Regex;

use super::model::DevrigConfig;

/// Compiled pattern matching `$VAR`, `${VAR}`, and `$$` escape sequences.
static ENV_VAR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)|\$\$")
        .expect("valid regex")
});

// ---------------------------------------------------------------------------
// SecretRegistry — tracks secret values for masking
// ---------------------------------------------------------------------------

/// Tracks expanded secret values so they can be masked in output.
#[derive(Debug, Default)]
pub struct SecretRegistry {
    secret_values: HashSet<String>,
}

impl SecretRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a value as secret (to be masked in output).
    pub fn track(&mut self, value: &str) {
        if !value.is_empty() {
            self.secret_values.insert(value.to_string());
        }
    }

    /// Replace any known secret substrings in `value` with `****`.
    pub fn mask_value(&self, value: &str) -> String {
        let mut result = value.to_string();
        for secret in &self.secret_values {
            if !secret.is_empty() {
                result = result.replace(secret.as_str(), "****");
            }
        }
        result
    }

    /// Check if the value contains any tracked secret.
    pub fn contains_secret(&self, value: &str) -> bool {
        self.secret_values
            .iter()
            .any(|s| !s.is_empty() && value.contains(s.as_str()))
    }
}

// ---------------------------------------------------------------------------
// .env file parser
// ---------------------------------------------------------------------------

/// Parse a `.env` file into key-value pairs.
///
/// Supports: `KEY=VALUE`, `KEY="VALUE"`, `KEY='VALUE'`, `# comments`, blank lines.
/// Returns an empty map if the file does not exist.
pub fn parse_env_file(path: &Path) -> Result<BTreeMap<String, String>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading .env file {}", path.display()))?;

    parse_env_content(&content)
}

fn parse_env_content(content: &str) -> Result<BTreeMap<String, String>> {
    let mut vars = BTreeMap::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Skip blank lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Split on first '='
        let Some((key, value)) = trimmed.split_once('=') else {
            bail!(
                ".env line {}: expected KEY=VALUE, got {:?}",
                line_num + 1,
                trimmed
            );
        };

        let key = key.trim().to_string();
        if key.is_empty() {
            bail!(".env line {}: empty key", line_num + 1);
        }

        let value = value.trim();

        // Strip matching quotes
        let value = if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            &value[1..value.len() - 1]
        } else {
            value
        };

        vars.insert(key, value.to_string());
    }

    Ok(vars)
}

// ---------------------------------------------------------------------------
// $VAR expansion engine
// ---------------------------------------------------------------------------

/// Expand `$VAR`, `${VAR}`, and `$$` escape sequences in a string.
///
/// Lookup order: (1) `env_file_vars`, (2) host process env via `std::env::var`.
/// Returns `(expanded_string, had_expansion)`.
pub fn expand_env_vars(
    input: &str,
    env_file_vars: &BTreeMap<String, String>,
    field_context: &str,
) -> Result<(String, bool)> {
    // Fast path: no $ in the string at all
    if !input.contains('$') {
        return Ok((input.to_string(), false));
    }

    let mut result = String::with_capacity(input.len());
    let mut last_end = 0;
    let mut had_expansion = false;

    for caps in ENV_VAR_RE.captures_iter(input) {
        let m = caps.get(0).unwrap();
        result.push_str(&input[last_end..m.start()]);

        let matched = m.as_str();
        if matched == "$$" {
            // Escape: $$ -> $
            result.push('$');
        } else {
            // Extract var name from ${VAR} or $VAR
            let var_name = caps
                .get(1)
                .or_else(|| caps.get(2))
                .unwrap()
                .as_str();

            // Lookup: env_file_vars first, then host env
            let value = env_file_vars
                .get(var_name)
                .cloned()
                .or_else(|| std::env::var(var_name).ok());

            match value {
                Some(val) => {
                    result.push_str(&val);
                    had_expansion = true;
                }
                None => {
                    bail!(
                        "undefined environment variable ${} in {}",
                        var_name,
                        field_context
                    );
                }
            }
        }

        last_end = m.end();
    }

    result.push_str(&input[last_end..]);
    Ok((result, had_expansion))
}

// ---------------------------------------------------------------------------
// Config walker — expand $VAR across config fields
// ---------------------------------------------------------------------------

/// Load .env files referenced in the config, returning a merged lookup pool.
pub fn load_env_files(
    config: &DevrigConfig,
    config_dir: &Path,
) -> Result<BTreeMap<String, String>> {
    let mut vars = BTreeMap::new();

    // Project-level env_file
    if let Some(env_file) = &config.project.env_file {
        let path = config_dir.join(env_file);
        let file_vars = parse_env_file(&path)
            .with_context(|| format!("loading project env_file {:?}", env_file))?;
        vars.extend(file_vars);
    }

    // Per-service env_file values
    for (name, svc) in &config.services {
        if let Some(env_file) = &svc.env_file {
            let path = config_dir.join(env_file);
            let file_vars = parse_env_file(&path)
                .with_context(|| format!("loading env_file for service {:?}", name))?;
            vars.extend(file_vars);
        }
    }

    Ok(vars)
}

/// Merge .env file values into config.env and per-service env maps.
/// .env values have lower priority than explicit TOML values.
pub fn merge_env_file_values(
    config: &mut DevrigConfig,
    config_dir: &Path,
) -> Result<()> {
    // Project-level env_file -> config.env (lower priority)
    if let Some(env_file) = &config.project.env_file {
        let path = config_dir.join(env_file);
        let file_vars = parse_env_file(&path)?;
        for (key, value) in file_vars {
            config.env.entry(key).or_insert(value);
        }
    }

    // Per-service env_file -> service.env (lower priority)
    for svc in config.services.values_mut() {
        if let Some(env_file) = &svc.env_file {
            let path = config_dir.join(env_file);
            let file_vars = parse_env_file(&path)?;
            for (key, value) in file_vars {
                svc.env.entry(key).or_insert(value);
            }
        }
    }

    Ok(())
}

/// Walk the config and expand `$VAR` references in all supported string fields.
/// Returns a `SecretRegistry` tracking which values came from expansion.
pub fn expand_config_env_vars(
    config: &mut DevrigConfig,
    env_file_vars: &BTreeMap<String, String>,
) -> Result<SecretRegistry> {
    let mut registry = SecretRegistry::new();

    // Global env values
    let pairs: Vec<(String, String)> = config.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    for (key, value) in pairs {
        let (expanded, was_secret) = expand_env_vars(&value, env_file_vars, &format!("env.{key}"))?;
        if was_secret {
            registry.track(&expanded);
        }
        config.env.insert(key, expanded);
    }

    // Service env values
    let svc_names: Vec<String> = config.services.keys().cloned().collect();
    for svc_name in svc_names {
        let pairs: Vec<(String, String)> = config.services[&svc_name]
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (key, value) in pairs {
            let (expanded, was_secret) = expand_env_vars(
                &value,
                env_file_vars,
                &format!("services.{svc_name}.env.{key}"),
            )?;
            if was_secret {
                registry.track(&expanded);
            }
            if let Some(svc) = config.services.get_mut(&svc_name) {
                svc.env.insert(key, expanded);
            }
        }
    }

    // Docker env values, image, and registry_auth
    let docker_names: Vec<String> = config.docker.keys().cloned().collect();
    for docker_name in docker_names {
        // docker.*.env values
        let pairs: Vec<(String, String)> = config.docker[&docker_name]
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (key, value) in pairs {
            let (expanded, was_secret) = expand_env_vars(
                &value,
                env_file_vars,
                &format!("docker.{docker_name}.env.{key}"),
            )?;
            if was_secret {
                registry.track(&expanded);
            }
            if let Some(docker) = config.docker.get_mut(&docker_name) {
                docker.env.insert(key, expanded);
            }
        }

        // docker.*.image
        let image = config.docker[&docker_name].image.clone();
        let (expanded, was_secret) = expand_env_vars(
            &image,
            env_file_vars,
            &format!("docker.{docker_name}.image"),
        )?;
        if was_secret {
            registry.track(&expanded);
        }
        if let Some(docker) = config.docker.get_mut(&docker_name) {
            docker.image = expanded;
        }

        // docker.*.registry_auth
        if let Some(auth) = config.docker[&docker_name].registry_auth.as_ref() {
            let username = auth.username.clone();
            let password = auth.password.clone();

            let (expanded_user, user_secret) = expand_env_vars(
                &username,
                env_file_vars,
                &format!("docker.{docker_name}.registry_auth.username"),
            )?;
            let (expanded_pass, pass_secret) = expand_env_vars(
                &password,
                env_file_vars,
                &format!("docker.{docker_name}.registry_auth.password"),
            )?;

            if user_secret {
                registry.track(&expanded_user);
            }
            if pass_secret {
                registry.track(&expanded_pass);
            }

            if let Some(auth_mut) = config
                .docker
                .get_mut(&docker_name)
                .and_then(|d| d.registry_auth.as_mut())
            {
                auth_mut.username = expanded_user;
                auth_mut.password = expanded_pass;
            }
        }
    }

    // Cluster registries
    if let Some(cluster) = &mut config.cluster {
        for (i, reg) in cluster.registries.iter_mut().enumerate() {
            let (expanded_url, _) = expand_env_vars(
                &reg.url,
                env_file_vars,
                &format!("cluster.registries[{}].url", i),
            )?;
            let (expanded_user, user_secret) = expand_env_vars(
                &reg.username,
                env_file_vars,
                &format!("cluster.registries[{}].username", i),
            )?;
            let (expanded_pass, pass_secret) = expand_env_vars(
                &reg.password,
                env_file_vars,
                &format!("cluster.registries[{}].password", i),
            )?;

            if user_secret {
                registry.track(&expanded_user);
            }
            if pass_secret {
                registry.track(&expanded_pass);
            }

            reg.url = expanded_url;
            reg.username = expanded_user;
            reg.password = expanded_pass;
        }
    }

    Ok(registry)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- .env parsing tests ---

    #[test]
    fn parse_env_basic() {
        let content = "FOO=bar\nBAZ=qux\n";
        let vars = parse_env_content(content).unwrap();
        assert_eq!(vars["FOO"], "bar");
        assert_eq!(vars["BAZ"], "qux");
    }

    #[test]
    fn parse_env_comments_and_blanks() {
        let content = "# this is a comment\n\nFOO=bar\n  # another comment\n\nBAZ=qux\n";
        let vars = parse_env_content(content).unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars["FOO"], "bar");
    }

    #[test]
    fn parse_env_double_quotes() {
        let content = "FOO=\"hello world\"\n";
        let vars = parse_env_content(content).unwrap();
        assert_eq!(vars["FOO"], "hello world");
    }

    #[test]
    fn parse_env_single_quotes() {
        let content = "FOO='hello world'\n";
        let vars = parse_env_content(content).unwrap();
        assert_eq!(vars["FOO"], "hello world");
    }

    #[test]
    fn parse_env_empty_value() {
        let content = "FOO=\n";
        let vars = parse_env_content(content).unwrap();
        assert_eq!(vars["FOO"], "");
    }

    #[test]
    fn parse_env_missing_file_returns_empty() {
        let vars = parse_env_file(Path::new("/nonexistent/.env")).unwrap();
        assert!(vars.is_empty());
    }

    // --- $VAR expansion tests ---

    #[test]
    fn expand_dollar_var() {
        let env = BTreeMap::from([("MY_KEY".to_string(), "secret123".to_string())]);
        let (result, expanded) = expand_env_vars("prefix-$MY_KEY-suffix", &env, "test").unwrap();
        assert_eq!(result, "prefix-secret123-suffix");
        assert!(expanded);
    }

    #[test]
    fn expand_braced_var() {
        let env = BTreeMap::from([("DB_PASS".to_string(), "p@ss".to_string())]);
        let (result, expanded) =
            expand_env_vars("postgres://user:${DB_PASS}@localhost", &env, "test").unwrap();
        assert_eq!(result, "postgres://user:p@ss@localhost");
        assert!(expanded);
    }

    #[test]
    fn expand_dollar_dollar_escape() {
        let env = BTreeMap::new();
        let (result, expanded) = expand_env_vars("price is $$5", &env, "test").unwrap();
        assert_eq!(result, "price is $5");
        assert!(!expanded);
    }

    #[test]
    fn expand_multiple_vars() {
        let env = BTreeMap::from([
            ("USER".to_string(), "admin".to_string()),
            ("PASS".to_string(), "secret".to_string()),
        ]);
        let (result, expanded) =
            expand_env_vars("$USER:$PASS@host", &env, "test").unwrap();
        assert_eq!(result, "admin:secret@host");
        assert!(expanded);
    }

    #[test]
    fn expand_undefined_var_errors() {
        let env = BTreeMap::new();
        // Clear this from process env to ensure it's not found
        let result = expand_env_vars("$DEFINITELY_UNDEFINED_VAR_12345", &env, "services.api.env.KEY");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("DEFINITELY_UNDEFINED_VAR_12345"));
        assert!(err.contains("services.api.env.KEY"));
    }

    #[test]
    fn expand_no_vars_is_noop() {
        let env = BTreeMap::new();
        let (result, expanded) =
            expand_env_vars("plain string with no vars", &env, "test").unwrap();
        assert_eq!(result, "plain string with no vars");
        assert!(!expanded);
    }

    #[test]
    fn expand_host_env_fallback() {
        // Use PATH which is set on both Unix and Windows
        let env = BTreeMap::new();
        let (result, expanded) = expand_env_vars("$PATH", &env, "test").unwrap();
        assert!(!result.is_empty());
        assert!(expanded);
    }

    #[test]
    fn expand_env_file_takes_priority_over_host() {
        let env = BTreeMap::from([("HOME".to_string(), "/custom/home".to_string())]);
        let (result, _) = expand_env_vars("$HOME", &env, "test").unwrap();
        assert_eq!(result, "/custom/home");
    }

    // --- SecretRegistry tests ---

    #[test]
    fn secret_registry_mask_value() {
        let mut reg = SecretRegistry::new();
        reg.track("secret123");
        reg.track("p@ssword");

        assert_eq!(
            reg.mask_value("url=postgres://user:secret123@localhost"),
            "url=postgres://user:****@localhost"
        );
        assert_eq!(
            reg.mask_value("password=p@ssword"),
            "password=****"
        );
    }

    #[test]
    fn secret_registry_contains_secret() {
        let mut reg = SecretRegistry::new();
        reg.track("mysecret");
        assert!(reg.contains_secret("prefix-mysecret-suffix"));
        assert!(!reg.contains_secret("no secrets here"));
    }

    #[test]
    fn secret_registry_empty_value_ignored() {
        let mut reg = SecretRegistry::new();
        reg.track("");
        assert!(!reg.contains_secret("anything"));
        assert_eq!(reg.mask_value("anything"), "anything");
    }

    // --- Config walker tests ---

    #[test]
    fn expand_config_env_vars_end_to_end() {
        use crate::config::model::*;

        let mut config = DevrigConfig {
            project: ProjectConfig {
                name: "test".to_string(),
                env_file: None,
            },
            services: BTreeMap::new(),
            docker: BTreeMap::new(),
            compose: None,
            cluster: None,
            dashboard: None,
            env: BTreeMap::from([("KEY".to_string(), "$TEST_SECRET".to_string())]),
            network: None,
        };

        let env_file_vars =
            BTreeMap::from([("TEST_SECRET".to_string(), "expanded_value".to_string())]);

        let registry = expand_config_env_vars(&mut config, &env_file_vars).unwrap();

        assert_eq!(config.env["KEY"], "expanded_value");
        assert!(registry.contains_secret("expanded_value"));
    }

    #[test]
    fn expand_config_preserves_non_secret_values() {
        use crate::config::model::*;

        let mut config = DevrigConfig {
            project: ProjectConfig {
                name: "test".to_string(),
                env_file: None,
            },
            services: BTreeMap::new(),
            docker: BTreeMap::new(),
            compose: None,
            cluster: None,
            dashboard: None,
            env: BTreeMap::from([("PLAIN".to_string(), "no-vars-here".to_string())]),
            network: None,
        };

        let env_file_vars = BTreeMap::new();
        let registry = expand_config_env_vars(&mut config, &env_file_vars).unwrap();

        assert_eq!(config.env["PLAIN"], "no-vars-here");
        assert!(!registry.contains_secret("no-vars-here"));
    }
}
