use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::cluster::K3dManager;
use crate::config;
use crate::config::resolve::resolve_config;
use crate::identity::ProjectIdentity;

pub async fn run_create(config_file: Option<&Path>) -> Result<()> {
    let config_path = resolve_config(config_file)?;
    let config = config::load_config(&config_path)?;
    let identity = ProjectIdentity::from_config(&config, &config_path)?;

    let cluster_config = config
        .cluster
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no [cluster] section in config"))?;

    let state_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".devrig");

    // Need network name - use the slug-based convention
    let network_name = format!("devrig-{}-net", identity.slug);

    let k3d_mgr = K3dManager::new(&identity.slug, cluster_config, &state_dir, &network_name);
    k3d_mgr
        .create_cluster()
        .await
        .context("creating k3d cluster")?;
    k3d_mgr
        .write_kubeconfig()
        .await
        .context("writing kubeconfig")?;

    println!("Cluster '{}' created", k3d_mgr.cluster_name());
    println!("Kubeconfig: {}", k3d_mgr.kubeconfig_path().display());
    Ok(())
}

pub async fn run_delete(config_file: Option<&Path>) -> Result<()> {
    let config_path = resolve_config(config_file)?;
    let config = config::load_config(&config_path)?;
    let identity = ProjectIdentity::from_config(&config, &config_path)?;

    let cluster_config = config
        .cluster
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no [cluster] section in config"))?;

    let state_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".devrig");

    let network_name = format!("devrig-{}-net", identity.slug);

    let k3d_mgr = K3dManager::new(&identity.slug, cluster_config, &state_dir, &network_name);
    k3d_mgr
        .delete_cluster()
        .await
        .context("deleting k3d cluster")?;

    println!("Cluster '{}' deleted", k3d_mgr.cluster_name());
    Ok(())
}

pub fn run_kubeconfig(config_file: Option<&Path>) -> Result<()> {
    let config_path = resolve_config(config_file)?;

    let state_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".devrig");

    let kubeconfig_path = state_dir.join("kubeconfig");
    if !kubeconfig_path.exists() {
        bail!(
            "kubeconfig not found -- is the cluster running? (expected: {})",
            kubeconfig_path.display()
        );
    }
    println!("{}", kubeconfig_path.display());
    Ok(())
}

pub async fn run_kubectl(config_file: Option<&Path>, args: Vec<String>) -> Result<()> {
    let config_path = resolve_config(config_file)?;

    let state_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".devrig");

    let kubeconfig_path = state_dir.join("kubeconfig");
    if !kubeconfig_path.exists() {
        bail!("kubeconfig not found -- is the cluster running? Start with `devrig start` first.");
    }

    // Spawn kubectl inheriting stdin/stdout/stderr for interactive use
    let status = std::process::Command::new("kubectl")
        .args(&args)
        .env("KUBECONFIG", &kubeconfig_path)
        .status()
        .context("running kubectl")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
