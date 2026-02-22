use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceEntry {
    pub slug: String,
    pub config_path: String,
    pub state_dir: String,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct InstanceRegistry {
    pub instances: Vec<InstanceEntry>,
}

impl InstanceRegistry {
    fn registry_path() -> PathBuf {
        crate::platform::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".devrig")
            .join("instances.json")
    }

    pub fn load() -> Self {
        let path = Self::registry_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::registry_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &content)?;
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    pub fn register(&mut self, entry: InstanceEntry) {
        // Update existing entry or add new one
        if let Some(existing) = self.instances.iter_mut().find(|e| e.slug == entry.slug) {
            *existing = entry;
        } else {
            self.instances.push(entry);
        }
    }

    pub fn unregister(&mut self, slug: &str) {
        self.instances.retain(|e| e.slug != slug);
    }

    pub fn list(&self) -> &[InstanceEntry] {
        &self.instances
    }

    /// Remove entries whose state files no longer exist
    pub fn cleanup(&mut self) {
        self.instances.retain(|entry| {
            let state_path = PathBuf::from(&entry.state_dir).join("state.json");
            state_path.exists()
        });
    }
}
