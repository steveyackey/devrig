use anyhow::{bail, Result};
use chrono::Utc;
use std::path::Path;
use std::time::SystemTime;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::config::model::{ClusterDeployConfig, ClusterImageConfig};
use crate::orchestrator::state::ClusterDeployState;

/// Run a subprocess command with optional working directory and environment variable,
/// racing the process against the cancellation token.
async fn run_cmd(
    cmd: &str,
    args: &[&str],
    working_dir: Option<&Path>,
    env: Option<(&str, &Path)>,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut command = Command::new(cmd);
    command.args(args);

    if let Some(dir) = working_dir {
        command.current_dir(dir);
    }

    if let Some((key, value)) = env {
        command.env(key, value);
    }

    let child = command.output();

    let output = tokio::select! {
        result = child => result?,
        _ = cancel.cancelled() => {
            bail!("cancelled");
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "{} {} failed: {}",
            cmd,
            args.first().unwrap_or(&""),
            stderr.trim()
        );
    }

    Ok(())
}

/// Build, push (if registry is available), and apply manifests for a cluster deploy entry.
/// Returns the deploy state with the image tag and timestamp.
pub async fn run_deploy(
    name: &str,
    deploy_config: &ClusterDeployConfig,
    registry_port: Option<u16>,
    kubeconfig_path: &Path,
    config_dir: &Path,
    cancel: &CancellationToken,
) -> Result<ClusterDeployState> {
    let context_path = config_dir.join(&deploy_config.context);
    let manifests_path = config_dir.join(&deploy_config.manifests);

    // Build the image tag
    let tag = if let Some(port) = registry_port {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        format!("localhost:{port}/{name}:{timestamp}")
    } else {
        format!("devrig-{name}:latest")
    };

    // Docker build
    debug!(name, tag, "building image");
    let dockerfile = &deploy_config.dockerfile;
    run_cmd(
        "docker",
        &["build", "-t", &tag, "-f", dockerfile, "."],
        Some(&context_path),
        None,
        cancel,
    )
    .await?;

    if cancel.is_cancelled() {
        bail!("cancelled");
    }

    // Docker push (only when registry is available)
    if registry_port.is_some() {
        debug!(name, tag, "pushing image");
        run_cmd("docker", &["push", &tag], None, None, cancel).await?;

        if cancel.is_cancelled() {
            bail!("cancelled");
        }
    }

    // kubectl apply
    let manifests_str = manifests_path.to_string_lossy();
    debug!(name, manifests = %manifests_str, "applying manifests");
    run_cmd(
        "kubectl",
        &["apply", "-f", &manifests_str],
        None,
        Some(("KUBECONFIG", kubeconfig_path)),
        cancel,
    )
    .await?;

    Ok(ClusterDeployState {
        image_tag: tag,
        last_deployed: Utc::now(),
    })
}

/// Rebuild: same as run_deploy but also restarts the deployment to pick up the new image.
pub async fn run_rebuild(
    name: &str,
    deploy_config: &ClusterDeployConfig,
    registry_port: Option<u16>,
    kubeconfig_path: &Path,
    config_dir: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    let context_path = config_dir.join(&deploy_config.context);
    let manifests_path = config_dir.join(&deploy_config.manifests);

    // Build the image tag
    let tag = if let Some(port) = registry_port {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        format!("localhost:{port}/{name}:{timestamp}")
    } else {
        format!("devrig-{name}:latest")
    };

    // Docker build
    debug!(name, tag, "rebuilding image");
    let dockerfile = &deploy_config.dockerfile;
    run_cmd(
        "docker",
        &["build", "-t", &tag, "-f", dockerfile, "."],
        Some(&context_path),
        None,
        cancel,
    )
    .await?;

    if cancel.is_cancelled() {
        bail!("cancelled");
    }

    // Docker push (only when registry is available)
    if registry_port.is_some() {
        debug!(name, tag, "pushing image");
        run_cmd("docker", &["push", &tag], None, None, cancel).await?;

        if cancel.is_cancelled() {
            bail!("cancelled");
        }
    }

    // kubectl apply
    let manifests_str = manifests_path.to_string_lossy();
    debug!(name, manifests = %manifests_str, "applying manifests");
    run_cmd(
        "kubectl",
        &["apply", "-f", &manifests_str],
        None,
        Some(("KUBECONFIG", kubeconfig_path)),
        cancel,
    )
    .await?;

    if cancel.is_cancelled() {
        bail!("cancelled");
    }

    // Rollout restart to pick up the new image
    let deployment = format!("deployment/{name}");
    debug!(name, "restarting deployment");
    run_cmd(
        "kubectl",
        &["rollout", "restart", &deployment],
        None,
        Some(("KUBECONFIG", kubeconfig_path)),
        cancel,
    )
    .await?;

    Ok(())
}

/// Build and push an image to the registry without applying any manifests.
/// Used for `[cluster.image.*]` entries that only need the image available.
pub async fn run_image_build(
    name: &str,
    image_config: &ClusterImageConfig,
    registry_port: Option<u16>,
    config_dir: &Path,
    cancel: &CancellationToken,
) -> Result<ClusterDeployState> {
    let context_path = config_dir.join(&image_config.context);

    // Build the image tag
    let tag = if let Some(port) = registry_port {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        format!("localhost:{port}/{name}:{timestamp}")
    } else {
        format!("devrig-{name}:latest")
    };

    // Docker build
    debug!(name, tag, "building image");
    let dockerfile = &image_config.dockerfile;
    run_cmd(
        "docker",
        &["build", "-t", &tag, "-f", dockerfile, "."],
        Some(&context_path),
        None,
        cancel,
    )
    .await?;

    if cancel.is_cancelled() {
        bail!("cancelled");
    }

    // Docker push (only when registry is available)
    if registry_port.is_some() {
        debug!(name, tag, "pushing image");
        run_cmd("docker", &["push", &tag], None, None, cancel).await?;
    }

    Ok(ClusterDeployState {
        image_tag: tag,
        last_deployed: Utc::now(),
    })
}

/// Rebuild an image and push it (no manifests, no rollout restart).
/// Used by the watcher for `[cluster.image.*]` entries with `watch = true`.
pub async fn rebuild_image(
    name: &str,
    image_config: &ClusterImageConfig,
    registry_port: Option<u16>,
    config_dir: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    let context_path = config_dir.join(&image_config.context);

    // Build the image tag
    let tag = if let Some(port) = registry_port {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        format!("localhost:{port}/{name}:{timestamp}")
    } else {
        format!("devrig-{name}:latest")
    };

    // Docker build
    debug!(name, tag, "rebuilding image");
    let dockerfile = &image_config.dockerfile;
    run_cmd(
        "docker",
        &["build", "-t", &tag, "-f", dockerfile, "."],
        Some(&context_path),
        None,
        cancel,
    )
    .await?;

    if cancel.is_cancelled() {
        bail!("cancelled");
    }

    // Docker push (only when registry is available)
    if registry_port.is_some() {
        debug!(name, tag, "pushing image");
        run_cmd("docker", &["push", &tag], None, None, cancel).await?;
    }

    Ok(())
}
