pub mod deploy;
pub mod registry;
pub mod watcher;

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{info, warn};

use crate::config::model::ClusterConfig;

/// Manages the lifecycle of a k3d Kubernetes cluster for a devrig project.
pub struct K3dManager {
    cluster_name: String,
    slug: String,
    kubeconfig_path: PathBuf,
    network_name: String,
    config: ClusterConfig,
}

impl K3dManager {
    /// Create a new K3dManager for the given project slug and cluster configuration.
    pub fn new(slug: &str, config: &ClusterConfig, state_dir: &Path, network_name: &str) -> Self {
        let cluster_name = format!("devrig-{}", slug);
        let kubeconfig_path = state_dir.join("kubeconfig");
        Self {
            cluster_name,
            slug: slug.to_string(),
            kubeconfig_path,
            network_name: network_name.to_string(),
            config: config.clone(),
        }
    }

    /// Create the k3d cluster if it does not already exist (idempotent).
    pub async fn create_cluster(&self) -> Result<()> {
        if self.cluster_exists().await? {
            info!(cluster = %self.cluster_name, "cluster already exists, skipping create");
            return Ok(());
        }

        let mut args = vec![
            "cluster".to_string(),
            "create".to_string(),
            self.cluster_name.clone(),
            "--network".to_string(),
            self.network_name.clone(),
            "--agents".to_string(),
            self.config.agents.to_string(),
            "--kubeconfig-update-default=false".to_string(),
            "--kubeconfig-switch-context=false".to_string(),
            "--api-port".to_string(),
            "127.0.0.1:0".to_string(),
        ];

        for entry in &self.config.ports {
            args.push("-p".to_string());
            args.push(entry.clone());
        }

        if self.config.registry {
            args.push("--registry-create".to_string());
            args.push(format!("k3d-{}-reg:0.0.0.0:0", self.cluster_name));
        }

        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.run_k3d(&arg_refs).await?;
        info!(cluster = %self.cluster_name, "cluster created");

        Ok(())
    }

    /// Delete the k3d cluster and remove the local kubeconfig file if it exists.
    pub async fn delete_cluster(&self) -> Result<()> {
        self.run_k3d(&["cluster", "delete", &self.cluster_name])
            .await?;
        info!(cluster = %self.cluster_name, "cluster deleted");

        if self.kubeconfig_path.exists() {
            tokio::fs::remove_file(&self.kubeconfig_path)
                .await
                .context("removing kubeconfig file")?;
        }

        Ok(())
    }

    /// Check whether the k3d cluster already exists.
    pub async fn cluster_exists(&self) -> Result<bool> {
        let output = self.run_k3d(&["cluster", "list", "-o", "json"]).await?;
        let clusters: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("parsing k3d cluster list JSON")?;
        let exists = clusters
            .iter()
            .any(|c| c.get("name").and_then(|n| n.as_str()) == Some(&self.cluster_name));
        Ok(exists)
    }

    /// Write the cluster kubeconfig to the local state directory.
    ///
    /// After writing, checks whether the kubeconfig contains an unresolved
    /// API server port (`:0`) — this happens when `--api-port 127.0.0.1:0`
    /// is used and k3d doesn't resolve the actual port. If detected, the
    /// actual port is discovered from the k3d serverlb Docker container and
    /// the kubeconfig is rewritten with the correct port.
    pub async fn write_kubeconfig(&self) -> Result<()> {
        let kubeconfig = self
            .run_k3d(&["kubeconfig", "get", &self.cluster_name])
            .await?;
        tokio::fs::write(&self.kubeconfig_path, kubeconfig.as_bytes())
            .await
            .context("writing kubeconfig file")?;

        // Fix unresolved port 0 if k3d didn't resolve it
        self.fix_kubeconfig_port().await?;

        info!(path = %self.kubeconfig_path.display(), "kubeconfig written");
        Ok(())
    }

    /// If the kubeconfig contains a server URL with port 0, discover the actual
    /// API server port from the k3d serverlb Docker container and fix it.
    async fn fix_kubeconfig_port(&self) -> Result<()> {
        let content = tokio::fs::read_to_string(&self.kubeconfig_path)
            .await
            .context("reading kubeconfig for port fix")?;

        // Check if any server line ends with :0
        let needs_fix = content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("server:") && trimmed.ends_with(":0")
        });

        if !needs_fix {
            return Ok(());
        }

        warn!("kubeconfig contains unresolved port 0, discovering actual API server port");

        // The k3d serverlb container proxies to the API server on port 6443.
        // Its name is k3d-{cluster_name}-serverlb.
        let container = format!("k3d-{}-serverlb", self.cluster_name);
        let output = Command::new("docker")
            .args([
                "inspect",
                &container,
                "--format",
                "{{(index .NetworkSettings.Ports \"6443/tcp\" 0).HostPort}}",
            ])
            .output()
            .await
            .context("inspecting serverlb container for API port")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "failed to discover API server port from '{}': {}",
                container,
                stderr.trim()
            );
        }

        let actual_port = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if actual_port.is_empty() || actual_port == "0" {
            bail!(
                "API server port could not be resolved (got '{}')",
                actual_port
            );
        }

        // Replace port 0 with actual port in server URLs
        let fixed = content
            .replace(
                "https://127.0.0.1:0",
                &format!("https://127.0.0.1:{}", actual_port),
            )
            .replace(
                "https://0.0.0.0:0",
                &format!("https://127.0.0.1:{}", actual_port),
            );

        tokio::fs::write(&self.kubeconfig_path, fixed.as_bytes())
            .await
            .context("writing fixed kubeconfig")?;

        info!(port = %actual_port, "fixed kubeconfig API server port");
        Ok(())
    }

    /// Run kubectl with the cluster kubeconfig, returning stdout on success.
    pub async fn kubectl(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("kubectl")
            .args(args)
            .env("KUBECONFIG", &self.kubeconfig_path)
            .output()
            .await
            .context("running kubectl")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "kubectl {} failed: {}",
                args.first().unwrap_or(&""),
                stderr.trim()
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Execute a k3d command, returning stdout on success or bailing with stderr.
    async fn run_k3d(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("k3d")
            .args(args)
            .output()
            .await
            .context("running k3d")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "k3d {} failed: {}",
                args.first().unwrap_or(&""),
                stderr.trim()
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Return the cluster name.
    pub fn cluster_name(&self) -> &str {
        &self.cluster_name
    }

    /// Return the path to the kubeconfig file.
    pub fn kubeconfig_path(&self) -> &Path {
        &self.kubeconfig_path
    }

    /// Return the Docker network name the cluster is attached to.
    pub fn network_name(&self) -> &str {
        &self.network_name
    }

    /// Return the project slug.
    pub fn slug(&self) -> &str {
        &self.slug
    }
}
