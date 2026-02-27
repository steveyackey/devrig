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
    pub protocol: Option<String>,
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
    #[serde(default)]
    pub protocol: Option<String>,
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

    /// Acquire an exclusive file lock on state.json.lock.
    /// Returns the lock file handle (lock released on drop).
    fn lock_state(state_dir: &Path) -> Option<std::fs::File> {
        let lock_path = state_dir.join("state.json.lock");
        let lock_file = std::fs::File::create(&lock_path).ok()?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            // SAFETY: fd is valid for the lifetime of lock_file
            unsafe {
                libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX);
            }
        }

        Some(lock_file)
    }

    /// Atomically update a single service's phase in state.json.
    ///
    /// Uses an exclusive file lock (`flock`) to prevent concurrent
    /// read-modify-write races when multiple grace timers fire at once.
    pub fn update_service_phase(state_dir: &Path, service: &str, phase: &str) {
        let _lock = Self::lock_state(state_dir);
        if let Some(mut state) = Self::load(state_dir) {
            if let Some(svc) = state.services.get_mut(service) {
                svc.phase = Some(phase.to_string());
            }
            let _ = state.save(state_dir);
        }
    }

    /// Atomically update a single service's PID in state.json.
    pub fn update_service_pid(state_dir: &Path, service: &str, pid: u32) {
        let _lock = Self::lock_state(state_dir);
        if let Some(mut state) = Self::load(state_dir) {
            if let Some(svc) = state.services.get_mut(service) {
                svc.pid = pid;
            }
            let _ = state.save(state_dir);
        }
    }

    /// Atomically update a service's phase and exit_code in state.json.
    pub fn update_service_exit(
        state_dir: &Path,
        service: &str,
        phase: &str,
        exit_code: Option<i32>,
    ) {
        let _lock = Self::lock_state(state_dir);
        if let Some(mut state) = Self::load(state_dir) {
            if let Some(svc) = state.services.get_mut(service) {
                svc.phase = Some(phase.to_string());
                svc.exit_code = exit_code;
            }
            let _ = state.save(state_dir);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

    fn test_state() -> ProjectState {
        let mut services = BTreeMap::new();
        services.insert(
            "api".to_string(),
            ServiceState {
                pid: 0,
                port: Some(3000),
                port_auto: false,
                protocol: None,
                phase: None,
                exit_code: None,
            },
        );
        ProjectState {
            slug: "test".to_string(),
            config_path: "devrig.toml".to_string(),
            services,
            started_at: Utc::now(),
            docker: BTreeMap::new(),
            compose_services: BTreeMap::new(),
            network_name: None,
            cluster: None,
            dashboard: None,
        }
    }

    #[test]
    fn update_service_pid_persists() {
        let dir = tempdir().unwrap();
        let state_dir = dir.path();

        let state = test_state();
        state.save(state_dir).unwrap();

        // PID starts at 0
        let loaded = ProjectState::load(state_dir).unwrap();
        assert_eq!(loaded.services["api"].pid, 0);

        // Update PID
        ProjectState::update_service_pid(state_dir, "api", 12345);

        // Reload and verify
        let loaded = ProjectState::load(state_dir).unwrap();
        assert_eq!(loaded.services["api"].pid, 12345);
    }
}
