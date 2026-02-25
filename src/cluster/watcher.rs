use anyhow::{Context, Result};
use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, error, warn};

use crate::cluster::deploy;
use crate::config::model::{ClusterDeployConfig, ClusterImageConfig};
use crate::orchestrator::state::ClusterDeployState;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".devrig",
    ".claude",
    "__pycache__",
];

const IGNORED_EXTENSIONS: &[&str] = &["swp", "swo", "tmp", "pyc", "pyo"];

/// Start file watchers for all cluster deploys that have `watch = true`.
///
/// Each watcher monitors the deploy's context directory for file changes,
/// debounces rapid edits, and triggers a rebuild+redeploy cycle.
pub async fn start_watchers(
    deploys: &BTreeMap<String, ClusterDeployConfig>,
    registry_port: Option<u16>,
    kubeconfig_path: PathBuf,
    config_dir: PathBuf,
    cancel: CancellationToken,
    tracker: &TaskTracker,
) -> Result<()> {
    for (name, deploy_config) in deploys {
        if !deploy_config.watch {
            continue;
        }

        let name = name.clone();
        let deploy_config = deploy_config.clone();
        let kubeconfig_path = kubeconfig_path.clone();
        let config_dir = config_dir.clone();
        let cancel = cancel.clone();

        tracker.spawn(async move {
            if let Err(e) = watch_and_rebuild(
                name.clone(),
                deploy_config,
                registry_port,
                kubeconfig_path,
                config_dir,
                cancel,
            )
            .await
            {
                error!(deploy = %name, error = %e, "watcher failed");
            }
        });
    }

    Ok(())
}

/// Start file watchers for all cluster images that have `watch = true`.
///
/// Each watcher monitors the image's context directory for file changes,
/// debounces rapid edits, and triggers a rebuild+push cycle (no manifests).
pub async fn start_image_watchers(
    images: &BTreeMap<String, ClusterImageConfig>,
    registry_port: Option<u16>,
    config_dir: PathBuf,
    deployed: BTreeMap<String, ClusterDeployState>,
    cancel: CancellationToken,
    tracker: &TaskTracker,
) -> Result<()> {
    for (name, image_config) in images {
        if !image_config.watch {
            continue;
        }

        let name = name.clone();
        let image_config = image_config.clone();
        let config_dir = config_dir.clone();
        let deployed = deployed.clone();
        let cancel = cancel.clone();

        tracker.spawn(async move {
            if let Err(e) = watch_and_rebuild_image(
                name.clone(),
                image_config,
                registry_port,
                config_dir,
                deployed,
                cancel,
            )
            .await
            {
                error!(image = %name, error = %e, "image watcher failed");
            }
        });
    }

    Ok(())
}

/// Watch a single image's context directory for file changes and trigger
/// rebuild+push cycles when relevant files are modified.
async fn watch_and_rebuild_image(
    name: String,
    image_config: ClusterImageConfig,
    registry_port: Option<u16>,
    config_dir: PathBuf,
    deployed: BTreeMap<String, ClusterDeployState>,
    cancel: CancellationToken,
) -> Result<()> {
    let watch_path = config_dir.join(&image_config.context);

    if !watch_path.exists() {
        warn!(
            image = %name,
            path = %watch_path.display(),
            "watch directory does not exist, skipping watcher"
        );
        return Ok(());
    }

    let (tx, mut rx) = mpsc::channel(100);

    let mut debouncer = new_debouncer(Duration::from_millis(500), move |result| {
        match result {
            Ok(events) => {
                if let Err(e) = tx.try_send(events) {
                    let _ = e;
                }
            }
            Err(e) => {
                eprintln!("file watcher error: {}", e);
            }
        }
    })
    .context("creating file watcher debouncer")?;

    debouncer
        .watcher()
        .watch(&watch_path, RecursiveMode::Recursive)
        .with_context(|| format!("watching directory {}", watch_path.display()))?;

    debug!(
        image = %name,
        path = %watch_path.display(),
        "image file watcher started"
    );

    let mut rebuild_cancel: Option<CancellationToken> = None;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!(image = %name, "image watcher shutting down");
                if let Some(token) = rebuild_cancel.take() {
                    token.cancel();
                }
                break;
            }
            events = rx.recv() => {
                let events = match events {
                    Some(events) => events,
                    None => {
                        warn!(image = %name, "image watcher channel closed unexpectedly");
                        break;
                    }
                };

                let relevant: Vec<_> = events
                    .iter()
                    .filter(|ev| ev.kind == DebouncedEventKind::Any)
                    .filter(|ev| !should_ignore(&ev.path))
                    .collect();

                if relevant.is_empty() {
                    continue;
                }

                debug!(
                    image = %name,
                    "file change detected, rebuilding image..."
                );

                if let Some(token) = rebuild_cancel.take() {
                    token.cancel();
                }

                let child_cancel = cancel.child_token();
                rebuild_cancel = Some(child_cancel.clone());

                let rebuild_name = name.clone();
                let rebuild_config = image_config.clone();
                let rebuild_config_dir = config_dir.clone();

                let rebuild_deployed = deployed.clone();
                tokio::spawn(async move {
                    match deploy::rebuild_image(
                        &rebuild_name,
                        &rebuild_config,
                        registry_port,
                        &rebuild_config_dir,
                        &rebuild_deployed,
                        &child_cancel,
                    )
                    .await
                    {
                        Ok(()) => {
                            debug!(image = %rebuild_name, "image rebuild completed successfully");
                        }
                        Err(e) => {
                            if child_cancel.is_cancelled() {
                                debug!(
                                    image = %rebuild_name,
                                    "image rebuild cancelled (newer change detected)"
                                );
                            } else {
                                error!(
                                    image = %rebuild_name,
                                    error = %e,
                                    "image rebuild failed"
                                );
                            }
                        }
                    }
                });
            }
        }
    }

    drop(debouncer);

    Ok(())
}

/// Watch a single deploy's context directory for file changes and trigger
/// rebuilds when relevant files are modified.
async fn watch_and_rebuild(
    name: String,
    deploy_config: ClusterDeployConfig,
    registry_port: Option<u16>,
    kubeconfig_path: PathBuf,
    config_dir: PathBuf,
    cancel: CancellationToken,
) -> Result<()> {
    let watch_path = config_dir.join(&deploy_config.context);

    if !watch_path.exists() {
        warn!(
            deploy = %name,
            path = %watch_path.display(),
            "watch directory does not exist, skipping watcher"
        );
        return Ok(());
    }

    let (tx, mut rx) = mpsc::channel(100);

    let mut debouncer = new_debouncer(Duration::from_millis(500), move |result| {
        match result {
            Ok(events) => {
                if let Err(e) = tx.try_send(events) {
                    // If the channel is full or closed, log will happen on the receiver side
                    let _ = e;
                }
            }
            Err(e) => {
                eprintln!("file watcher error: {}", e);
            }
        }
    })
    .context("creating file watcher debouncer")?;

    debouncer
        .watcher()
        .watch(&watch_path, RecursiveMode::Recursive)
        .with_context(|| format!("watching directory {}", watch_path.display()))?;

    debug!(
        deploy = %name,
        path = %watch_path.display(),
        "file watcher started"
    );

    // Track any in-progress rebuild so we can cancel it on new changes.
    let mut rebuild_cancel: Option<CancellationToken> = None;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!(deploy = %name, "watcher shutting down");
                // Cancel any in-progress rebuild.
                if let Some(token) = rebuild_cancel.take() {
                    token.cancel();
                }
                // Drop the debouncer by breaking out of the loop; the local
                // variable is dropped when the function returns.
                break;
            }
            events = rx.recv() => {
                let events = match events {
                    Some(events) => events,
                    None => {
                        // Channel closed -- debouncer was dropped unexpectedly.
                        warn!(deploy = %name, "watcher channel closed unexpectedly");
                        break;
                    }
                };

                // Filter to only relevant events (non-ignored paths with
                // an actual data-change kind).
                let relevant: Vec<_> = events
                    .iter()
                    .filter(|ev| ev.kind == DebouncedEventKind::Any)
                    .filter(|ev| !should_ignore(&ev.path))
                    .collect();

                if relevant.is_empty() {
                    continue;
                }

                debug!(
                    deploy = %name,
                    "file change detected, rebuilding..."
                );

                // Cancel any previous in-progress rebuild.
                if let Some(token) = rebuild_cancel.take() {
                    token.cancel();
                }

                // Create a child cancellation token for this rebuild so it
                // can be cancelled independently when the next change arrives.
                let child_cancel = cancel.child_token();
                rebuild_cancel = Some(child_cancel.clone());

                let rebuild_name = name.clone();
                let rebuild_config = deploy_config.clone();
                let rebuild_kubeconfig = kubeconfig_path.clone();
                let rebuild_config_dir = config_dir.clone();

                tokio::spawn(async move {
                    match deploy::run_rebuild(
                        &rebuild_name,
                        &rebuild_config,
                        registry_port,
                        &rebuild_kubeconfig,
                        &rebuild_config_dir,
                        &child_cancel,
                    )
                    .await
                    {
                        Ok(()) => {
                            debug!(deploy = %rebuild_name, "rebuild completed successfully");
                        }
                        Err(e) => {
                            if child_cancel.is_cancelled() {
                                debug!(
                                    deploy = %rebuild_name,
                                    "rebuild cancelled (newer change detected)"
                                );
                            } else {
                                error!(
                                    deploy = %rebuild_name,
                                    error = %e,
                                    "rebuild failed"
                                );
                            }
                        }
                    }
                });
            }
        }
    }

    // Explicitly drop to silence unused-variable warnings and make intent clear.
    drop(debouncer);

    Ok(())
}

/// Returns `true` if the given path should be ignored by the file watcher.
///
/// A path is ignored if any of its directory components match an entry in
/// `IGNORED_DIRS`, or if its file extension matches an entry in
/// `IGNORED_EXTENSIONS`.
fn should_ignore(path: &Path) -> bool {
    for component in path.components() {
        if let std::path::Component::Normal(segment) = component {
            if let Some(s) = segment.to_str() {
                if IGNORED_DIRS.contains(&s) {
                    return true;
                }
            }
        }
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if IGNORED_EXTENSIONS.contains(&ext) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_should_ignore_git_dir() {
        assert!(should_ignore(Path::new("src/.git/config")));
        assert!(should_ignore(Path::new(".git/HEAD")));
    }

    #[test]
    fn test_should_ignore_target_dir() {
        assert!(should_ignore(Path::new("target/debug/build")));
    }

    #[test]
    fn test_should_ignore_node_modules() {
        assert!(should_ignore(Path::new(
            "frontend/node_modules/react/index.js"
        )));
    }

    #[test]
    fn test_should_ignore_pycache() {
        assert!(should_ignore(Path::new(
            "app/__pycache__/module.cpython-310.pyc"
        )));
    }

    #[test]
    fn test_should_ignore_devrig_dir() {
        assert!(should_ignore(Path::new(".devrig/state.json")));
    }

    #[test]
    fn test_should_ignore_claude_dir() {
        assert!(should_ignore(Path::new(".claude/settings.json")));
    }

    #[test]
    fn test_should_ignore_swap_files() {
        assert!(should_ignore(Path::new("src/main.rs.swp")));
        assert!(should_ignore(Path::new("src/main.rs.swo")));
    }

    #[test]
    fn test_should_ignore_tmp_files() {
        assert!(should_ignore(Path::new("data/output.tmp")));
    }

    #[test]
    fn test_should_ignore_pyc_files() {
        assert!(should_ignore(Path::new("app/module.pyc")));
        assert!(should_ignore(Path::new("app/module.pyo")));
    }

    #[test]
    fn test_should_not_ignore_normal_files() {
        assert!(!should_ignore(Path::new("src/main.rs")));
        assert!(!should_ignore(Path::new("Cargo.toml")));
        assert!(!should_ignore(Path::new("frontend/src/App.tsx")));
        assert!(!should_ignore(Path::new("Dockerfile")));
    }
}
