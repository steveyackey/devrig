pub mod graph;
pub mod ports;
pub mod registry;
pub mod state;
pub mod supervisor;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{error, info, warn};

use crate::config;
use crate::config::model::{DevrigConfig, Port};
use crate::config::validate::validate;
use crate::identity::ProjectIdentity;
use crate::ui::logs::{LogLine, LogWriter};
use crate::ui::summary::{print_startup_summary, RunningService};

use graph::DependencyResolver;
use ports::{check_all_ports, find_free_port, format_port_conflicts};
use registry::{InstanceEntry, InstanceRegistry};
use state::{ProjectState, ServiceState};
use supervisor::{RestartPolicy, ServiceSupervisor};

/// Central orchestrator that loads configuration, resolves dependencies,
/// spawns supervised services, and manages graceful shutdown.
pub struct Orchestrator {
    config: DevrigConfig,
    identity: ProjectIdentity,
    config_path: PathBuf,
    state_dir: PathBuf,
    cancel: CancellationToken,
    tracker: TaskTracker,
}

impl Orchestrator {
    /// Create an Orchestrator from a config file path.
    ///
    /// Loads and parses the config, validates it, and computes the project
    /// identity and state directory.
    pub fn from_config(config_path: PathBuf) -> Result<Self> {
        let config = config::load_config(&config_path)
            .with_context(|| format!("loading config from {}", config_path.display()))?;

        if let Err(errors) = validate(&config) {
            let mut msg = String::from("Configuration errors:\n");
            for err in &errors {
                msg.push_str(&format!("  - {}\n", err));
            }
            bail!("{}", msg.trim_end());
        }

        let identity = ProjectIdentity::from_config(&config, &config_path)
            .context("computing project identity")?;

        let state_dir = config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(".devrig");

        Ok(Self {
            config,
            identity,
            config_path,
            state_dir,
            cancel: CancellationToken::new(),
            tracker: TaskTracker::new(),
        })
    }

    /// Start services according to the configuration.
    ///
    /// If `service_filter` is non-empty, only the named services (plus their
    /// transitive dependencies) are started.
    pub async fn start(&self, service_filter: Vec<String>) -> Result<()> {
        // --- Resolve dependency order ---
        let resolver =
            DependencyResolver::from_config(&self.config).map_err(|e| anyhow::anyhow!("{}", e))?;
        let full_order = resolver
            .start_order()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // --- Filter to requested services + transitive deps ---
        let launch_order = if service_filter.is_empty() {
            full_order
        } else {
            // Validate that all requested services exist.
            for name in &service_filter {
                if !self.config.services.contains_key(name) {
                    bail!(
                        "unknown service '{}' (available: {:?})",
                        name,
                        self.config.services.keys().collect::<Vec<_>>()
                    );
                }
            }

            // Collect transitive dependencies.
            let mut needed: std::collections::HashSet<String> =
                service_filter.iter().cloned().collect();
            let mut changed = true;
            while changed {
                changed = false;
                let snapshot: Vec<String> = needed.iter().cloned().collect();
                for name in &snapshot {
                    if let Some(svc) = self.config.services.get(name) {
                        for dep in &svc.depends_on {
                            if needed.insert(dep.clone()) {
                                changed = true;
                            }
                        }
                    }
                }
            }

            // Keep only needed services, preserving dependency order.
            full_order
                .into_iter()
                .filter(|name| needed.contains(name))
                .collect()
        };

        if launch_order.is_empty() {
            bail!("no services to start");
        }

        // --- Check port conflicts ---
        let conflicts = check_all_ports(&self.config.services);
        if !conflicts.is_empty() {
            bail!("{}", format_port_conflicts(&conflicts));
        }

        // --- Resolve auto ports ---
        let mut resolved_ports: BTreeMap<String, Option<u16>> = BTreeMap::new();
        for name in &launch_order {
            let svc = &self.config.services[name];
            let port = match &svc.port {
                Some(Port::Fixed(p)) => Some(*p),
                Some(Port::Auto) => Some(find_free_port()),
                None => None,
            };
            resolved_ports.insert(name.clone(), port);
        }

        // --- Create state directory ---
        std::fs::create_dir_all(&self.state_dir)
            .with_context(|| format!("creating state dir {}", self.state_dir.display()))?;

        // --- Compute max service name length for log formatting ---
        let max_name_len = launch_order.iter().map(|n| n.len()).max().unwrap_or(0);

        // --- Create log channel and spawn LogWriter ---
        let (log_tx, log_rx) = mpsc::channel::<LogLine>(1024);
        let log_writer = LogWriter::new(log_rx, max_name_len);
        self.tracker.spawn(async move {
            log_writer.run().await;
        });

        // --- Spawn supervisors in dependency order ---
        for name in &launch_order {
            let svc = &self.config.services[name];

            // Merge env: global env + service env + PORT
            let mut env: BTreeMap<String, String> = self.config.env.clone();
            for (k, v) in &svc.env {
                env.insert(k.clone(), v.clone());
            }
            if let Some(port) = resolved_ports.get(name).copied().flatten() {
                env.insert("PORT".to_string(), port.to_string());
            }

            // Resolve working directory relative to the config file location.
            let working_dir = svc.path.as_ref().map(|p| {
                let base = self
                    .config_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."));
                base.join(p)
            });

            let supervisor = ServiceSupervisor::new(
                name.clone(),
                svc.command.clone(),
                working_dir,
                env,
                RestartPolicy::default(),
                log_tx.clone(),
                self.cancel.clone(),
            );

            let svc_name = name.clone();
            self.tracker.spawn(async move {
                match supervisor.run().await {
                    Ok(status) => {
                        info!(service = %svc_name, %status, "supervisor finished");
                    }
                    Err(e) => {
                        // Cancellation errors are expected during shutdown.
                        if !e.to_string().contains("cancelled") {
                            error!(service = %svc_name, error = %e, "supervisor failed");
                        }
                    }
                }
            });
        }

        // Drop our copy of the log sender so LogWriter can detect when all
        // supervisors are done.
        drop(log_tx);

        // --- Build and save ProjectState ---
        let mut service_states: BTreeMap<String, ServiceState> = BTreeMap::new();
        for name in &launch_order {
            let svc = &self.config.services[name];
            let port = resolved_ports.get(name).copied().flatten();
            let port_auto = matches!(&svc.port, Some(Port::Auto));
            service_states.insert(
                name.clone(),
                ServiceState {
                    pid: 0, // PIDs are not readily available from async supervisors
                    port,
                    port_auto,
                },
            );
        }

        let project_state = ProjectState {
            slug: self.identity.slug.clone(),
            config_path: self.config_path.to_string_lossy().to_string(),
            services: service_states,
            started_at: Utc::now(),
            infra: BTreeMap::new(),
            compose_services: BTreeMap::new(),
            network_name: None,
        };
        project_state
            .save(&self.state_dir)
            .context("saving project state")?;

        // --- Register in global instance registry ---
        let mut registry = InstanceRegistry::load();
        registry.register(InstanceEntry {
            slug: self.identity.slug.clone(),
            config_path: self.config_path.to_string_lossy().to_string(),
            state_dir: self.state_dir.to_string_lossy().to_string(),
            started_at: Utc::now(),
        });
        if let Err(e) = registry.save() {
            warn!(error = %e, "failed to save instance registry");
        }

        // --- Print startup summary ---
        let mut summary_services: BTreeMap<String, RunningService> = BTreeMap::new();
        for name in &launch_order {
            let svc = &self.config.services[name];
            let port = resolved_ports.get(name).copied().flatten();
            let port_auto = matches!(&svc.port, Some(Port::Auto));
            summary_services.insert(
                name.clone(),
                RunningService {
                    port,
                    port_auto,
                    status: "running".to_string(),
                },
            );
        }
        print_startup_summary(&self.identity, &summary_services);

        // --- Wait for shutdown signal or all tasks to exit ---
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nShutting down...");
            }
            _ = async {
                self.tracker.close();
                self.tracker.wait().await;
            } => {
                eprintln!("All services exited");
            }
        }

        // --- Graceful shutdown (stop, not delete) ---
        self.cancel.cancel();
        self.tracker.close();
        match tokio::time::timeout(std::time::Duration::from_secs(10), self.tracker.wait()).await {
            Ok(()) => info!("All services stopped cleanly"),
            Err(_) => warn!("Shutdown timed out -- some processes may have been force-killed"),
        }

        // State and registry are preserved on stop (Ctrl+C).
        // Only `devrig delete` removes state and unregisters.

        Ok(())
    }

    /// Stop a running project by cancelling all tasks.
    /// Preserves state and registry so `devrig ps` still sees it.
    pub async fn stop(&self) -> Result<()> {
        let _state = ProjectState::load(&self.state_dir)
            .context("no running project state found -- is the project running?")?;

        self.cancel.cancel();
        self.tracker.close();
        match tokio::time::timeout(std::time::Duration::from_secs(10), self.tracker.wait()).await {
            Ok(()) => info!("All services stopped cleanly"),
            Err(_) => warn!("Shutdown timed out -- some processes may have been force-killed"),
        }

        // State and registry are preserved on stop.

        Ok(())
    }

    /// Stop the project, remove state, and unregister from the global registry.
    pub async fn delete(&self) -> Result<()> {
        // Stop first (ignore errors if nothing is running).
        let _ = self.stop().await;

        // Remove project state file.
        if let Err(e) = ProjectState::remove(&self.state_dir) {
            warn!(error = %e, "failed to remove project state");
        }

        // Remove the state directory entirely.
        if self.state_dir.exists() {
            std::fs::remove_dir_all(&self.state_dir).with_context(|| {
                format!("removing state directory {}", self.state_dir.display())
            })?;
        }

        // Unregister from global registry.
        let mut registry = InstanceRegistry::load();
        registry.unregister(&self.identity.slug);
        if let Err(e) = registry.save() {
            warn!(error = %e, "failed to save instance registry after unregister");
        }

        Ok(())
    }
}
