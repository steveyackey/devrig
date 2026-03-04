use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::Path;
use tokio_util::sync::CancellationToken;

use crate::cluster::deploy::{fresh_rebuild_deploy, fresh_rebuild_image};
use crate::cluster::registry::get_registry_port;
use crate::cluster::K3dManager;
use crate::config;
use crate::config::resolve::resolve_config;
use crate::identity::ProjectIdentity;
use crate::orchestrator::graph::{DependencyResolver, ResourceKind};
use crate::orchestrator::state::ClusterDeployState;

pub async fn run_create(config_file: Option<&Path>) -> Result<()> {
    let config_path = resolve_config(config_file)?;
    let (config, _source) = config::load_config(&config_path)?;
    let identity = ProjectIdentity::from_config(&config, &config_path)?;

    let cluster_config = config
        .cluster
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no [cluster] section in config"))?;

    let config_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let state_dir = config_dir.join(".devrig");

    // Need network name - use the slug-based convention
    let network_name = format!("devrig-{}-net", identity.slug);

    let k3d_mgr = K3dManager::new(&identity.slug, cluster_config, &state_dir, &network_name, config_dir);
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
    let (config, _source) = config::load_config(&config_path)?;
    let identity = ProjectIdentity::from_config(&config, &config_path)?;

    let cluster_config = config
        .cluster
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no [cluster] section in config"))?;

    let config_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let state_dir = config_dir.join(".devrig");

    let network_name = format!("devrig-{}-net", identity.slug);

    let k3d_mgr = K3dManager::new(&identity.slug, cluster_config, &state_dir, &network_name, config_dir);
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

/// Rebuild and re-push cluster images with --no-cache for a completely fresh build.
/// Respects dependency order via depends_on fields.
pub async fn run_rebuild_images(
    images: Vec<String>,
    no_apply: bool,
    config_file: Option<&Path>,
) -> Result<()> {
    let config_path = resolve_config(config_file)?;
    let (config, _source) = config::load_config(&config_path)?;
    let identity = ProjectIdentity::from_config(&config, &config_path)?;

    let cluster_config = config
        .cluster
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no [cluster] section in config"))?;

    let config_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let state_dir = config_dir.join(".devrig");

    // Check that the kubeconfig exists (cluster must be running)
    let kubeconfig_path = state_dir.join("kubeconfig");
    if !kubeconfig_path.exists() {
        bail!(
            "No k3d cluster is running (kubeconfig not found at {}). \
             Start the cluster first with `devrig cluster create` or `devrig start`.",
            kubeconfig_path.display()
        );
    }

    // Discover the registry port (cluster must have a registry)
    let registry_port = get_registry_port(&identity.slug)
        .await
        .context(
            "Could not find k3d registry. Is the cluster running? \
             Start with `devrig cluster create` or `devrig start`.",
        )?;

    // Collect all image and deploy names
    let all_image_names: Vec<String> = cluster_config.images.keys().cloned().collect();
    let all_deploy_names: Vec<String> = cluster_config.deploy.keys().cloned().collect();

    // If specific images were requested, validate they all exist
    if !images.is_empty() {
        for name in &images {
            if !cluster_config.images.contains_key(name)
                && !cluster_config.deploy.contains_key(name)
            {
                bail!(
                    "Unknown image or deploy '{}'. Available: {}",
                    name,
                    all_image_names
                        .iter()
                        .chain(all_deploy_names.iter())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    }

    // Build dependency graph and get topological order
    let resolver = DependencyResolver::from_config(&config)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let full_order = resolver
        .start_order()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Filter to only ClusterImage and ClusterDeploy entries
    let cluster_order: Vec<(String, ResourceKind)> = full_order
        .into_iter()
        .filter(|(_, kind)| {
            matches!(kind, ResourceKind::ClusterImage | ResourceKind::ClusterDeploy)
        })
        .collect();

    // Further filter if specific image names were requested
    let rebuild_order: Vec<(String, ResourceKind)> = if images.is_empty() {
        cluster_order
    } else {
        cluster_order
            .into_iter()
            .filter(|(name, _)| images.contains(name))
            .collect()
    };

    if rebuild_order.is_empty() {
        println!("No cluster images or deploys to rebuild.");
        return Ok(());
    }

    println!(
        "Rebuilding {} image(s) with --no-cache...",
        rebuild_order.len()
    );

    let cancel = CancellationToken::new();
    let mut deployed: BTreeMap<String, ClusterDeployState> = BTreeMap::new();

    for (name, kind) in &rebuild_order {
        match kind {
            ResourceKind::ClusterImage => {
                let image_config = &cluster_config.images[name];
                let state = fresh_rebuild_image(
                    name,
                    image_config,
                    registry_port,
                    config_dir,
                    &deployed,
                    &cancel,
                )
                .await
                .with_context(|| format!("rebuilding image '{}'", name))?;
                deployed.insert(name.clone(), state);
            }
            ResourceKind::ClusterDeploy => {
                let deploy_config = &cluster_config.deploy[name];
                let state = fresh_rebuild_deploy(
                    name,
                    deploy_config,
                    registry_port,
                    &kubeconfig_path,
                    config_dir,
                    !no_apply,
                    &cancel,
                )
                .await
                .with_context(|| format!("rebuilding deploy '{}'", name))?;
                deployed.insert(name.clone(), state);
            }
            _ => {}
        }
    }

    println!("All images rebuilt successfully.");
    Ok(())
}
