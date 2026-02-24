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
    #[serde(default)]
    pub docker: BTreeMap<String, DockerState>,
    #[serde(default)]
    pub compose_services: BTreeMap<String, ComposeServiceState>,
    #[serde(default)]
    pub network_name: Option<String>,
    #[serde(default)]
    pub cluster: Option<ClusterState>,
    #[serde(default)]
    pub dashboard: Option<DashboardState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterState {
    pub cluster_name: String,
    pub kubeconfig_path: String,
    pub registry_name: Option<String>,
    pub registry_port: Option<u16>,
    pub deployed_services: BTreeMap<String, ClusterDeployState>,
    #[serde(default)]
    pub installed_addons: BTreeMap<String, AddonState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterDeployState {
    pub image_tag: String,
    pub last_deployed: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonState {
    pub addon_type: String,
    pub namespace: String,
    pub installed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    pub pid: u32,
    pub port: Option<u16>,
    pub port_auto: bool,
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerState {
    pub container_id: String,
    pub container_name: String,
    pub port: Option<u16>,
    pub port_auto: bool,
    pub named_ports: BTreeMap<String, u16>,
    pub init_completed: bool,
    pub init_completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeServiceState {
    pub container_id: String,
    pub container_name: String,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardState {
    pub dashboard_port: u16,
    pub grpc_port: u16,
    pub http_port: u16,
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

    pub fn reset_init(&mut self, docker_name: &str) -> bool {
        if let Some(state) = self.docker.get_mut(docker_name) {
            state.init_completed = false;
            state.init_completed_at = None;
            true
        } else {
            false
        }
    }
}
