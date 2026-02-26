use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, error, warn};

use std::collections::HashMap;

use crate::config::model::AddonConfig;
use crate::config::interpolate::resolve_template;
use crate::orchestrator::state::AddonState;

// ---------------------------------------------------------------------------
// Helm value conversion
// ---------------------------------------------------------------------------

/// Convert a TOML value to a string suitable for `helm --set key=value`.
pub fn toml_value_to_helm_set(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(toml_value_to_helm_set).collect();
            format!("{{{}}}", items.join(","))
        }
        toml::Value::Table(_) | toml::Value::Datetime(_) => value.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Helm/kubectl command helpers
// ---------------------------------------------------------------------------

/// Run a helm command with the given args and KUBECONFIG env var.
async fn run_helm(args: &[&str], kubeconfig: &Path, cancel: &CancellationToken) -> Result<String> {
    let child = Command::new("helm")
        .args(args)
        .env("KUBECONFIG", kubeconfig)
        .output();

    let output = tokio::select! {
        result = child => result.context("running helm")?,
        _ = cancel.cancelled() => bail!("cancelled"),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "helm {} failed: {}",
            args.first().unwrap_or(&""),
            stderr.trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run a kubectl command with the given args and KUBECONFIG env var.
async fn run_kubectl(
    args: &[&str],
    kubeconfig: &Path,
    cancel: &CancellationToken,
) -> Result<String> {
    let child = Command::new("kubectl")
        .args(args)
        .env("KUBECONFIG", kubeconfig)
        .output();

    let output = tokio::select! {
        result = child => result.context("running kubectl")?,
        _ = cancel.cancelled() => bail!("cancelled"),
    };

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

// ---------------------------------------------------------------------------
// Individual addon installers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn install_helm_addon(
    name: &str,
    chart: &str,
    repo: Option<&str>,
    namespace: &str,
    version: Option<&str>,
    values: &BTreeMap<String, toml::Value>,
    values_files: &[String],
    wait: bool,
    timeout: &str,
    skip_crds: bool,
    kubeconfig: &Path,
    config_dir: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    // Resolve chart reference: OCI registry, remote repo, or local path
    let resolved_chart = if chart.starts_with("oci://") {
        // OCI chart — use directly, no repo add/update needed
        chart.to_string()
    } else if let Some(repo_url) = repo {
        // Derive the repo name from the chart reference (e.g. "fluxcd-community/flux2"
        // → "fluxcd-community"). Fall back to the addon name if there's no slash.
        let repo_name = chart
            .split('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(name);

        // Remote chart — add and update repo
        run_helm(
            &["repo", "add", repo_name, repo_url, "--force-update"],
            kubeconfig,
            cancel,
        )
        .await
        .with_context(|| format!("adding helm repo for addon '{}'", name))?;

        run_helm(&["repo", "update", repo_name], kubeconfig, cancel)
            .await
            .with_context(|| format!("updating helm repo for addon '{}'", name))?;

        chart.to_string()
    } else {
        // Local chart — resolve relative to config dir
        let chart_path = if Path::new(chart).is_absolute() {
            std::path::PathBuf::from(chart)
        } else {
            config_dir.join(chart)
        };
        if !chart_path.exists() {
            bail!(
                "local helm chart path '{}' does not exist (resolved from '{}')",
                chart_path.display(),
                chart
            );
        }
        chart_path.to_string_lossy().to_string()
    };

    // Build install args
    let mut args: Vec<String> = vec![
        "upgrade".to_string(),
        "--install".to_string(),
        name.to_string(),
        resolved_chart,
        "--namespace".to_string(),
        namespace.to_string(),
        "--create-namespace".to_string(),
    ];

    if skip_crds {
        args.push("--skip-crds".to_string());
    }

    if wait {
        args.push("--wait".to_string());
        args.push("--timeout".to_string());
        args.push(timeout.to_string());
    }

    if let Some(v) = version {
        args.push("--version".to_string());
        args.push(v.to_string());
    }

    // Add -f for each values file
    for vf in values_files {
        let vf_path = if Path::new(vf).is_absolute() {
            std::path::PathBuf::from(vf)
        } else {
            config_dir.join(vf)
        };
        args.push("-f".to_string());
        args.push(vf_path.to_string_lossy().to_string());
    }

    // Add --set for each value
    for (k, v) in values {
        args.push("--set".to_string());
        args.push(format!("{}={}", k, toml_value_to_helm_set(v)));
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_helm(&arg_refs, kubeconfig, cancel)
        .await
        .with_context(|| format!("installing helm addon '{}'", name))?;

    debug!(addon = %name, chart = %chart, "helm addon installed");
    Ok(())
}

async fn install_manifest_addon(
    name: &str,
    path: &str,
    namespace: Option<&str>,
    kubeconfig: &Path,
    config_dir: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    let manifest_path = if Path::new(path).is_absolute() {
        std::path::PathBuf::from(path)
    } else {
        config_dir.join(path)
    };
    let manifest_str = manifest_path.to_string_lossy().to_string();

    let mut args = vec!["apply", "-f", &manifest_str];
    let ns_str;
    if let Some(ns) = namespace {
        ns_str = ns.to_string();
        args.push("--namespace");
        args.push(&ns_str);
    }

    run_kubectl(&args, kubeconfig, cancel)
        .await
        .with_context(|| format!("applying manifest addon '{}'", name))?;

    debug!(addon = %name, path = %path, "manifest addon installed");
    Ok(())
}

async fn install_kustomize_addon(
    name: &str,
    path: &str,
    namespace: Option<&str>,
    kubeconfig: &Path,
    config_dir: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    let kustomize_path = if Path::new(path).is_absolute() {
        std::path::PathBuf::from(path)
    } else {
        config_dir.join(path)
    };
    let kustomize_str = kustomize_path.to_string_lossy().to_string();

    let mut args = vec!["apply", "-k", &kustomize_str];
    let ns_str;
    if let Some(ns) = namespace {
        ns_str = ns.to_string();
        args.push("--namespace");
        args.push(&ns_str);
    }

    run_kubectl(&args, kubeconfig, cancel)
        .await
        .with_context(|| format!("applying kustomize addon '{}'", name))?;

    debug!(addon = %name, path = %path, "kustomize addon installed");
    Ok(())
}

// ---------------------------------------------------------------------------
// Topological sort for addon install ordering
// ---------------------------------------------------------------------------

/// Topologically sort addons by `depends_on` using Kahn's algorithm.
///
/// Tie-breaking is alphabetical (deterministic). Returns an error if a cycle
/// is detected.
pub fn topo_sort_addons(addons: &BTreeMap<String, AddonConfig>) -> Result<Vec<String>> {
    // Build in-degree map and adjacency list
    let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();

    for name in addons.keys() {
        in_degree.entry(name.as_str()).or_insert(0);
    }

    for (name, addon) in addons {
        for dep in addon.depends_on() {
            // Only count deps that are actually in the addons map
            if addons.contains_key(dep.as_str()) {
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .insert(name.as_str());
                *in_degree.entry(name.as_str()).or_insert(0) += 1;
            }
        }
    }

    // Seed the queue with nodes that have zero in-degree (BTreeSet for alphabetical order)
    let mut ready: BTreeSet<&str> = BTreeSet::new();
    for (&name, &deg) in &in_degree {
        if deg == 0 {
            ready.insert(name);
        }
    }

    let mut sorted: Vec<String> = Vec::with_capacity(addons.len());

    while let Some(&name) = ready.iter().next() {
        ready.remove(name);
        sorted.push(name.to_string());

        if let Some(deps) = dependents.get(name) {
            for &dependent in deps {
                let deg = in_degree.get_mut(dependent).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    ready.insert(dependent);
                }
            }
        }
    }

    if sorted.len() != addons.len() {
        // Find cycle participants
        let in_cycle: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg > 0)
            .map(|(&name, _)| name.to_string())
            .collect();
        bail!(
            "addon dependency cycle detected involving: {}",
            in_cycle.join(", ")
        );
    }

    Ok(sorted)
}

// ---------------------------------------------------------------------------
// Bulk install/uninstall
// ---------------------------------------------------------------------------

/// Resolve `{{ }}` templates in the string values of a TOML values map.
fn resolve_values_templates(
    values: &BTreeMap<String, toml::Value>,
    template_vars: &HashMap<String, String>,
    addon_name: &str,
) -> Result<BTreeMap<String, toml::Value>> {
    let mut resolved = BTreeMap::new();
    for (key, value) in values {
        let resolved_val = match value {
            toml::Value::String(s) => {
                let field_ctx = format!("cluster.addons.{addon_name}.values.{key}");
                match resolve_template(s, template_vars, &field_ctx) {
                    Ok(r) => toml::Value::String(r),
                    Err(errs) => {
                        let msgs: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
                        bail!("{}", msgs.join("; "));
                    }
                }
            }
            other => other.clone(),
        };
        resolved.insert(key.clone(), resolved_val);
    }
    Ok(resolved)
}

/// Install all addons in dependency order (topological sort, alphabetical tie-break).
/// Returns a map of addon states for persistence.
pub async fn install_addons(
    addons: &BTreeMap<String, AddonConfig>,
    template_vars: &HashMap<String, String>,
    kubeconfig: &Path,
    config_dir: &Path,
    cancel: &CancellationToken,
) -> Result<BTreeMap<String, AddonState>> {
    let mut states = BTreeMap::new();
    let install_order = topo_sort_addons(addons)?;

    for name in &install_order {
        let addon = &addons[name];
        debug!(addon = %name, type_ = %addon.addon_type(), "installing addon");

        match addon {
            AddonConfig::Helm {
                chart,
                repo,
                namespace,
                version,
                values,
                values_files,
                wait,
                timeout,
                skip_crds,
                ..
            } => {
                let resolved_values =
                    resolve_values_templates(values, template_vars, name)?;
                install_helm_addon(
                    name,
                    chart,
                    repo.as_deref(),
                    namespace,
                    version.as_deref(),
                    &resolved_values,
                    values_files,
                    *wait,
                    timeout,
                    *skip_crds,
                    kubeconfig,
                    config_dir,
                    cancel,
                )
                .await?;
                states.insert(
                    name.clone(),
                    AddonState {
                        addon_type: "helm".to_string(),
                        namespace: namespace.clone(),
                        installed_at: Utc::now(),
                    },
                );
            }
            AddonConfig::Manifest {
                path, namespace, ..
            } => {
                install_manifest_addon(
                    name,
                    path,
                    namespace.as_deref(),
                    kubeconfig,
                    config_dir,
                    cancel,
                )
                .await?;
                states.insert(
                    name.clone(),
                    AddonState {
                        addon_type: "manifest".to_string(),
                        namespace: namespace.as_deref().unwrap_or("default").to_string(),
                        installed_at: Utc::now(),
                    },
                );
            }
            AddonConfig::Kustomize {
                path, namespace, ..
            } => {
                install_kustomize_addon(
                    name,
                    path,
                    namespace.as_deref(),
                    kubeconfig,
                    config_dir,
                    cancel,
                )
                .await?;
                states.insert(
                    name.clone(),
                    AddonState {
                        addon_type: "kustomize".to_string(),
                        namespace: namespace.as_deref().unwrap_or("default").to_string(),
                        installed_at: Utc::now(),
                    },
                );
            }
        }
    }

    Ok(states)
}

/// Uninstall all addons. Errors are logged but do not stop other uninstalls.
pub async fn uninstall_addons(
    addons: &BTreeMap<String, AddonConfig>,
    kubeconfig: &Path,
    config_dir: &Path,
    cancel: &CancellationToken,
) {
    // Uninstall in reverse dependency order (dependents first)
    let uninstall_order: Vec<String> = match topo_sort_addons(addons) {
        Ok(order) => order.into_iter().rev().collect(),
        Err(_) => addons.keys().rev().cloned().collect(), // fallback to reverse-alpha
    };

    for name in &uninstall_order {
        let addon = &addons[name];
        debug!(addon = %name, "uninstalling addon");
        let result = match addon {
            AddonConfig::Helm { namespace, .. } => {
                run_helm(
                    &["uninstall", name, "--namespace", namespace],
                    kubeconfig,
                    cancel,
                )
                .await
            }
            AddonConfig::Manifest {
                path, namespace, ..
            } => {
                let manifest_path = if Path::new(path.as_str()).is_absolute() {
                    std::path::PathBuf::from(path)
                } else {
                    config_dir.join(path)
                };
                let manifest_str = manifest_path.to_string_lossy().to_string();
                let mut args = vec!["delete", "-f", &manifest_str, "--ignore-not-found"];
                let ns_str;
                if let Some(ns) = namespace.as_deref() {
                    ns_str = ns.to_string();
                    args.push("--namespace");
                    args.push(&ns_str);
                }
                run_kubectl(&args, kubeconfig, cancel).await
            }
            AddonConfig::Kustomize {
                path, namespace, ..
            } => {
                let kustomize_path = if Path::new(path.as_str()).is_absolute() {
                    std::path::PathBuf::from(path)
                } else {
                    config_dir.join(path)
                };
                let kustomize_str = kustomize_path.to_string_lossy().to_string();
                let mut args = vec!["delete", "-k", &kustomize_str, "--ignore-not-found"];
                let ns_str;
                if let Some(ns) = namespace.as_deref() {
                    ns_str = ns.to_string();
                    args.push("--namespace");
                    args.push(&ns_str);
                }
                run_kubectl(&args, kubeconfig, cancel).await
            }
        };

        if let Err(e) = result {
            warn!(addon = %name, error = %e, "failed to uninstall addon");
        }
    }
}

// ---------------------------------------------------------------------------
// Port-forward manager
// ---------------------------------------------------------------------------

/// Manages port-forward processes for addon UIs.
pub struct PortForwardManager {
    tracker: TaskTracker,
    cancel: CancellationToken,
}

impl Default for PortForwardManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PortForwardManager {
    /// Create a new PortForwardManager.
    pub fn new() -> Self {
        Self {
            tracker: TaskTracker::new(),
            cancel: CancellationToken::new(),
        }
    }

    /// Start port-forwards for all addons that have port_forward entries.
    pub fn start_port_forwards(&self, addons: &BTreeMap<String, AddonConfig>, kubeconfig: &Path) {
        for (name, addon) in addons {
            let namespace = addon.namespace().unwrap_or("default").to_string();

            for (port_str, target) in addon.port_forward() {
                let local_port = match port_str.parse::<u16>() {
                    Ok(p) => p,
                    Err(_) => {
                        warn!(addon = %name, port = %port_str, "invalid port-forward port, skipping");
                        continue;
                    }
                };

                // Parse target: "svc/name:port" -> ("svc/name", "port")
                let (resource, remote_port) = match target.rsplit_once(':') {
                    Some((r, p)) => (r.to_string(), p.to_string()),
                    None => {
                        warn!(addon = %name, target = %target, "invalid port-forward target, expected resource:port");
                        continue;
                    }
                };

                let cancel = self.cancel.clone();
                let kubeconfig = kubeconfig.to_path_buf();
                let addon_name = name.clone();
                let ns = namespace.clone();

                self.tracker.spawn(async move {
                    let mut backoff = Duration::from_secs(1);
                    let max_backoff = Duration::from_secs(30);

                    loop {
                        debug!(
                            addon = %addon_name,
                            local_port = local_port,
                            target = format!("{}:{}", resource, remote_port),
                            "starting port-forward"
                        );

                        let mut child = match Command::new("kubectl")
                            .args([
                                "port-forward",
                                "--namespace",
                                &ns,
                                "--address",
                                "127.0.0.1",
                                &resource,
                                &format!("{}:{}", local_port, remote_port),
                            ])
                            .env("KUBECONFIG", &kubeconfig)
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::piped())
                            .kill_on_drop(true)
                            .spawn()
                        {
                            Ok(child) => child,
                            Err(e) => {
                                error!(addon = %addon_name, error = %e, "failed to spawn port-forward");
                                break;
                            }
                        };

                        let stderr_handle = child.stderr.take();
                        let started = Instant::now();

                        tokio::select! {
                            status = child.wait() => {
                                // Read captured stderr for a concise reason.
                                let reason = if let Some(mut stderr) = stderr_handle {
                                    let mut buf = String::new();
                                    let _ = stderr.read_to_string(&mut buf).await;
                                    if !buf.is_empty() {
                                        debug!(
                                            addon = %addon_name,
                                            stderr = %buf.trim(),
                                            "kubectl port-forward stderr"
                                        );
                                    }
                                    // Extract the last "error: ..." line as a concise reason.
                                    buf.lines()
                                        .rev()
                                        .find(|l| l.starts_with("error:"))
                                        .map(|l| l.trim_start_matches("error:").trim().to_string())
                                } else {
                                    None
                                };

                                match status {
                                    Ok(s) => {
                                        warn!(
                                            addon = %addon_name,
                                            local_port = local_port,
                                            exit = %s,
                                            reason = reason.as_deref().unwrap_or("unknown"),
                                            "port-forward exited, reconnecting in {:?}",
                                            backoff
                                        );
                                    }
                                    Err(e) => {
                                        warn!(
                                            addon = %addon_name,
                                            error = %e,
                                            reason = reason.as_deref().unwrap_or("unknown"),
                                            "port-forward error, reconnecting in {:?}",
                                            backoff
                                        );
                                    }
                                }

                                tokio::time::sleep(backoff).await;

                                // Reset backoff if the connection was stable (>60s).
                                if started.elapsed() > Duration::from_secs(60) {
                                    backoff = Duration::from_secs(1);
                                } else {
                                    backoff = (backoff * 2).min(max_backoff);
                                }
                            }
                            _ = cancel.cancelled() => {
                                let _ = child.kill().await;
                                debug!(addon = %addon_name, local_port = local_port, "port-forward stopped");
                                break;
                            }
                        }
                    }
                });
            }
        }
    }

    /// Stop all port-forward processes.
    pub async fn stop(&self) {
        self.cancel.cancel();
        self.tracker.close();
        match tokio::time::timeout(Duration::from_secs(5), self.tracker.wait()).await {
            Ok(()) => debug!("all port-forwards stopped"),
            Err(_) => warn!("port-forward shutdown timed out"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_value_string() {
        let val = toml::Value::String("hello".to_string());
        assert_eq!(toml_value_to_helm_set(&val), "hello");
    }

    #[test]
    fn toml_value_bool_true() {
        let val = toml::Value::Boolean(true);
        assert_eq!(toml_value_to_helm_set(&val), "true");
    }

    #[test]
    fn toml_value_bool_false() {
        let val = toml::Value::Boolean(false);
        assert_eq!(toml_value_to_helm_set(&val), "false");
    }

    #[test]
    fn toml_value_integer() {
        let val = toml::Value::Integer(42);
        assert_eq!(toml_value_to_helm_set(&val), "42");
    }

    #[test]
    fn toml_value_float() {
        let val = toml::Value::Float(3.14);
        assert_eq!(toml_value_to_helm_set(&val), "3.14");
    }

    #[test]
    fn toml_value_array() {
        let val = toml::Value::Array(vec![
            toml::Value::String("a".to_string()),
            toml::Value::String("b".to_string()),
            toml::Value::String("c".to_string()),
        ]);
        assert_eq!(toml_value_to_helm_set(&val), "{a,b,c}");
    }

    /// Helper to build a minimal Manifest addon for topo-sort tests.
    fn manifest_addon(deps: Vec<&str>) -> AddonConfig {
        AddonConfig::Manifest {
            path: "./test.yaml".to_string(),
            namespace: None,
            port_forward: BTreeMap::new(),
            depends_on: deps.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn topo_sort_no_deps_is_alphabetical() {
        let mut addons = BTreeMap::new();
        addons.insert("charlie".to_string(), manifest_addon(vec![]));
        addons.insert("alpha".to_string(), manifest_addon(vec![]));
        addons.insert("bravo".to_string(), manifest_addon(vec![]));

        let order = topo_sort_addons(&addons).unwrap();
        assert_eq!(order, vec!["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn topo_sort_respects_depends_on() {
        let mut addons = BTreeMap::new();
        addons.insert("app".to_string(), manifest_addon(vec!["cert-manager"]));
        addons.insert("cert-manager".to_string(), manifest_addon(vec![]));

        let order = topo_sort_addons(&addons).unwrap();
        assert_eq!(order, vec!["cert-manager", "app"]);
    }

    #[test]
    fn topo_sort_diamond_deps() {
        let mut addons = BTreeMap::new();
        addons.insert("d".to_string(), manifest_addon(vec!["b", "c"]));
        addons.insert("b".to_string(), manifest_addon(vec!["a"]));
        addons.insert("c".to_string(), manifest_addon(vec!["a"]));
        addons.insert("a".to_string(), manifest_addon(vec![]));

        let order = topo_sort_addons(&addons).unwrap();
        // a must come before b and c; b and c before d
        let pos = |n: &str| order.iter().position(|x| x == n).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("a") < pos("c"));
        assert!(pos("b") < pos("d"));
        assert!(pos("c") < pos("d"));
    }

    #[test]
    fn topo_sort_detects_cycle() {
        let mut addons = BTreeMap::new();
        addons.insert("a".to_string(), manifest_addon(vec!["b"]));
        addons.insert("b".to_string(), manifest_addon(vec!["a"]));

        let result = topo_sort_addons(&addons);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn topo_sort_ignores_external_deps() {
        // depends_on referencing non-addon names should be silently ignored
        let mut addons = BTreeMap::new();
        addons.insert("app".to_string(), manifest_addon(vec!["external-thing"]));

        let order = topo_sort_addons(&addons).unwrap();
        assert_eq!(order, vec!["app"]);
    }
}
