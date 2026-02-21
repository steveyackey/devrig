use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use crate::config::model::DevrigConfig;

#[derive(Debug, Clone)]
pub struct ProjectIdentity {
    pub name: String,
    pub id: String,
    pub slug: String,
    pub config_path: PathBuf,
}

/// Compute a project ID by hashing the given path string with SHA-256
/// and returning the first 8 hex characters.
///
/// This function does NOT canonicalize the path -- it hashes whatever
/// string `path.to_string_lossy()` produces.  The caller is responsible
/// for canonicalizing first when that is desired.
pub fn compute_project_id(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let hash = hasher.finalize();
    hex::encode(&hash[..4])
}

impl ProjectIdentity {
    /// Build a `ProjectIdentity` from a parsed config and its file path.
    ///
    /// The config path is canonicalized so that the resulting project ID
    /// is stable regardless of how the path was originally expressed
    /// (relative, with symlinks, etc.).
    pub fn from_config(config: &DevrigConfig, config_path: &Path) -> anyhow::Result<Self> {
        let canonical = config_path.canonicalize()?;
        let name = config.project.name.clone();
        let id = compute_project_id(&canonical);
        let slug = format!("{name}-{id}");

        Ok(Self {
            name,
            id,
            slug,
            config_path: canonical,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        let path = Path::new("/tmp/some/project");
        let id1 = compute_project_id(path);
        let id2 = compute_project_id(path);
        assert_eq!(id1, id2, "same path must always produce the same hash");
    }

    #[test]
    fn hash_is_8_hex_chars() {
        let id = compute_project_id(Path::new("/tmp/whatever"));
        assert_eq!(id.len(), 8, "id should be exactly 8 characters");
        assert!(
            id.chars().all(|c| c.is_ascii_hexdigit()),
            "id should contain only hex digits, got: {id}"
        );
    }

    #[test]
    fn different_paths_produce_different_hashes() {
        let id_a = compute_project_id(Path::new("/project/alpha"));
        let id_b = compute_project_id(Path::new("/project/beta"));
        assert_ne!(
            id_a, id_b,
            "different paths should (almost certainly) hash differently"
        );
    }

    #[test]
    fn slug_format() {
        let path = Path::new("/tmp/my-project/devrig.toml");
        let id = compute_project_id(path);
        let name = "myapp";
        let slug = format!("{name}-{id}");

        assert!(
            slug.starts_with("myapp-"),
            "slug should start with the project name followed by a dash"
        );
        assert_eq!(
            slug,
            format!("myapp-{id}"),
            "slug should be exactly '{{name}}-{{id}}'"
        );
    }
}
