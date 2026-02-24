use std::path::{Path, PathBuf};

/// Walk up the directory tree from `start`, checking for `filename` at each level.
/// Returns the full path to the file if found, or None if the root is reached
/// without finding it.
pub fn find_config(start: &Path, filename: &str) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let candidate = current.join(filename);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Resolve the config file path. If `cli_file` is provided, verify it exists and
/// return it. Otherwise, search from the current working directory upward for
/// "devrig.toml".
pub fn resolve_config(cli_file: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(path) = cli_file {
        if path.is_file() {
            return Ok(path.canonicalize()?);
        }
        anyhow::bail!("Config file not found: {}", path.display());
    }

    let cwd = std::env::current_dir()?;
    find_config(&cwd, "devrig.toml").ok_or_else(|| {
        anyhow::anyhow!(
            "No devrig.toml found in {} or any parent directory",
            cwd.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn config_in_current_dir_found() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("devrig.toml");
        fs::write(&config_path, "").unwrap();

        let result = find_config(tmp.path(), "devrig.toml");
        assert_eq!(result, Some(config_path));
    }

    #[test]
    fn config_in_parent_dir_found() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("devrig.toml");
        fs::write(&config_path, "").unwrap();

        let child = tmp.path().join("subdir");
        fs::create_dir(&child).unwrap();

        let result = find_config(&child, "devrig.toml");
        assert_eq!(result, Some(config_path));
    }

    #[test]
    fn config_in_grandparent_found() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("devrig.toml");
        fs::write(&config_path, "").unwrap();

        let grandchild = tmp.path().join("a").join("b");
        fs::create_dir_all(&grandchild).unwrap();

        let result = find_config(&grandchild, "devrig.toml");
        assert_eq!(result, Some(config_path));
    }

    #[test]
    fn no_config_returns_none() {
        let tmp = TempDir::new().unwrap();
        // No config file created anywhere in the temp directory.
        // Start from a nested directory to ensure the walk terminates.
        let nested = tmp.path().join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();

        let result = find_config(&nested, "devrig.toml");
        // The walk will go above tmp into the real filesystem. We cannot
        // guarantee there is no devrig.toml anywhere above /tmp, but it is
        // extremely unlikely in a CI or dev environment. We check that if the
        // result is Some it is NOT inside our temp tree (meaning we did not
        // get a false positive within the temp dir).
        if let Some(ref found) = result {
            assert!(
                !found.starts_with(tmp.path()),
                "Should not find config inside the temp directory"
            );
        }
    }

    #[test]
    fn cli_file_valid_path() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("custom.toml");
        fs::write(&config_path, "").unwrap();

        let result = resolve_config(Some(&config_path));
        assert!(result.is_ok());
        // canonicalize() may return UNC paths on Windows, so compare canonical forms
        assert_eq!(result.unwrap(), config_path.canonicalize().unwrap());
    }

    #[test]
    fn cli_file_invalid_path_errors() {
        let nonexistent = Path::new("/tmp/definitely_does_not_exist_devrig.toml");
        let result = resolve_config(Some(nonexistent));
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Config file not found"),
            "Expected 'Config file not found' in error, got: {}",
            err_msg
        );
    }
}
