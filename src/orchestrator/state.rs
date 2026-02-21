use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    pub slug: String,
    pub config_path: String,
    pub services: BTreeMap<String, ServiceState>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    pub pid: u32,
    pub port: Option<u16>,
    pub port_auto: bool,
}

impl ProjectState {
    pub fn save(&self, state_dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(state_dir)?;
        let path = state_dir.join("state.json");
        let content = serde_json::to_string_pretty(self)?;
        // Atomic write: write to tmp file then rename
        let tmp_path = state_dir.join("state.json.tmp");
        std::fs::write(&tmp_path, &content)?;
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    pub fn load(state_dir: &Path) -> Option<Self> {
        let path = state_dir.join("state.json");
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn remove(state_dir: &Path) -> anyhow::Result<()> {
        let path = state_dir.join("state.json");
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        // Try to remove the directory if it's empty
        let _ = std::fs::remove_dir(state_dir);
        Ok(())
    }

    pub fn state_dir_for(project_dir: &Path) -> std::path::PathBuf {
        project_dir.join(".devrig")
    }
}
