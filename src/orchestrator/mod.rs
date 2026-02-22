pub mod graph;
pub mod ports;
pub mod registry;
pub mod state;
pub mod supervisor;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{error, info, warn};

use crate::compose;
use crate::config;
use crate::config::interpolate::{build_template_vars, resolve_config_templates};
use crate::config::model::{DevrigConfig, Port};
use crate::config::validate::validate;
use crate::discovery::env::build_service_env;
use crate::identity::ProjectIdentity;
use crate::infra::InfraManager;
use crate::ui::logs::{LogLine, LogWriter};
use crate::ui::summary::{print_startup_summary, RunningService};

use graph::{DependencyResolver, ResourceKind};
use ports::{check_all_ports_unified, format_port_conflicts, resolve_port};
use registry::{InstanceEntry, InstanceRegistry};
use state::{ComposeServiceState, InfraState, ProjectState, ServiceState};
use supervisor::{RestartPolicy, ServiceSupervisor};

/// Central orchestrator that loads configuration, resolves dependencies,
/// manages Docker infrastructure, spawns supervised services, and handles
/// graceful shutdown.
///
/// Multi-phase startup order:
///   Phase 0 — Parse config, validate, load previous state
///   Phase 1 — Create Docker network (if infra/compose present)
///   Phase 2 — Compose up, bridge to network, ready checks
///   Phase 3 — Start infra containers in dependency order
///   Phase 4 — Resolve ports, templates, DEVRIG_* env vars
///   Phase 5 — Spawn service supervisors with injected env
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
    /// transitive dependencies including infra/compose) are started.
    pub async fn start(&mut self, service_filter: Vec<String>) -> Result<()> {
        // ================================================================
        // Phase 0: Parse, validate, resolve dependencies, load prev state
        // ================================================================
        let resolver =
            DependencyResolver::from_config(&self.config).map_err(|e| anyhow::anyhow!("{}", e))?;
        let full_order = resolver
            .start_order()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let prev_state = ProjectState::load(&self.state_dir);

        // Filter to requested services + transitive deps (across all resource types)
        let launch_order = if service_filter.is_empty() {
            full_order
        } else {
            for name in &service_filter {
                if !self.config.services.contains_key(name) {
                    bail!(
                        "unknown service '{}' (available: {:?})",
                        name,
                        self.config.services.keys().collect::<Vec<_>>()
                    );
                }
            }

            let mut needed: HashSet<String> = service_filter.iter().cloned().collect();
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
                    if let Some(infra) = self.config.infra.get(name) {
                        for dep in &infra.depends_on {
                            if needed.insert(dep.clone()) {
                                changed = true;
                            }
                        }
                    }
                }
            }

            full_order
                .into_iter()
                .filter(|(name, _)| needed.contains(name))
                .collect()
        };

        if launch_order.is_empty() {
            bail!("no resources to start");
        }

        // Check port conflicts for all fixed ports (services + infra)
        let conflicts = check_all_ports_unified(&self.config);
        if !conflicts.is_empty() {
            bail!("{}", format_port_conflicts(&conflicts));
        }

        // Create state directory
        std::fs::create_dir_all(&self.state_dir)
            .with_context(|| format!("creating state dir {}", self.state_dir.display()))?;

        let has_docker = launch_order
            .iter()
            .any(|(_, k)| matches!(k, ResourceKind::Infra | ResourceKind::Compose));

        // ================================================================
        // Phase 1: Docker network
        // ================================================================
        let infra_mgr = if has_docker {
            let mgr = InfraManager::new(self.identity.slug.clone()).await?;
            mgr.ensure_network().await?;
            info!(network = %mgr.network_name(), "Docker network ensured");
            Some(mgr)
        } else {
            None
        };

        let network_name = infra_mgr.as_ref().map(|m| m.network_name());

        // ================================================================
        // Phase 2: Compose services
        // ================================================================
        let mut compose_states: BTreeMap<String, ComposeServiceState> = BTreeMap::new();

        if let Some(compose_config) = &self.config.compose {
            let compose_file = self
                .config_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join(&compose_config.file);

            let compose_services: Vec<String> = launch_order
                .iter()
                .filter(|(_, k)| *k == ResourceKind::Compose)
                .map(|(n, _)| n.clone())
                .collect();

            if !compose_services.is_empty() {
                info!(services = ?compose_services, "starting compose services");
                compose::lifecycle::compose_up(
                    &compose_file,
                    &self.identity.slug,
                    &compose_services,
                    compose_config.env_file.as_deref(),
                )
                .await?;

                let containers =
                    compose::lifecycle::compose_ps(&compose_file, &self.identity.slug).await?;

                // Bridge compose containers to the devrig network
                if let Some(mgr) = &infra_mgr {
                    compose::bridge::bridge_compose_containers(
                        mgr.docker(),
                        &mgr.network_name(),
                        &containers,
                    )
                    .await?;
                }

                // Record compose service states
                for cs in &containers {
                    if compose_services.contains(&cs.service) {
                        compose_states.insert(
                            cs.service.clone(),
                            ComposeServiceState {
                                container_id: cs.id.clone(),
                                container_name: cs.name.clone(),
                                port: cs.publishers.first().map(|p| p.published_port),
                            },
                        );
                    }
                }

                info!(count = compose_states.len(), "compose services started");
            }
        }

        // ================================================================
        // Phase 3: Infrastructure containers (in dependency order)
        // ================================================================
        let mut infra_states: BTreeMap<String, InfraState> = BTreeMap::new();
        let mut allocated_ports: HashSet<u16> = HashSet::new();

        // Pre-populate allocated ports from compose services
        for cs in compose_states.values() {
            if let Some(port) = cs.port {
                allocated_ports.insert(port);
            }
        }

        for (name, kind) in &launch_order {
            if *kind != ResourceKind::Infra {
                continue;
            }

            let infra_config = self
                .config
                .infra
                .get(name)
                .ok_or_else(|| anyhow::anyhow!("infra '{}' not found in config", name))?
                .clone();

            let prev_infra = prev_state.as_ref().and_then(|s| s.infra.get(name));

            info!(infra = %name, image = %infra_config.image, "starting infra service");

            let state = infra_mgr
                .as_ref()
                .expect("infra_mgr must exist when infra resources are present")
                .start_service(name, &infra_config, prev_infra, &mut allocated_ports)
                .await
                .with_context(|| format!("starting infra service '{}'", name))?;

            infra_states.insert(name.clone(), state);
        }

        // ================================================================
        // Phase 4: Resolve ports, templates, and env vars
        // ================================================================
        let mut resolved_ports: HashMap<String, u16> = HashMap::new();

        // Infra ports
        for (name, state) in &infra_states {
            if let Some(port) = state.port {
                resolved_ports.insert(format!("infra:{}", name), port);
            }
            for (pname, &port) in &state.named_ports {
                resolved_ports.insert(format!("infra:{}:{}", name, pname), port);
            }
        }

        // Compose service ports
        for (name, state) in &compose_states {
            if let Some(port) = state.port {
                resolved_ports.insert(format!("compose:{}", name), port);
            }
        }

        // Service ports (with sticky auto-port support)
        for (name, kind) in &launch_order {
            if *kind != ResourceKind::Service {
                continue;
            }

            let svc = &self.config.services[name];
            if let Some(port_config) = &svc.port {
                let prev_port = prev_state
                    .as_ref()
                    .and_then(|s| s.services.get(name))
                    .and_then(|s| s.port);
                let prev_auto = prev_state
                    .as_ref()
                    .and_then(|s| s.services.get(name))
                    .map(|s| s.port_auto)
                    .unwrap_or(false);

                let port = resolve_port(
                    &format!("service:{}", name),
                    port_config,
                    prev_port,
                    prev_auto,
                    &mut allocated_ports,
                );
                resolved_ports.insert(format!("service:{}", name), port);
            }
        }

        // Build template variables and resolve {{ }} expressions in config
        let mut template_vars = build_template_vars(&self.config, &resolved_ports);

        // Add compose ports to template vars
        for (name, state) in &compose_states {
            if let Some(port) = state.port {
                template_vars.insert(format!("compose.{}.port", name), port.to_string());
            }
        }

        if let Err(errors) = resolve_config_templates(&mut self.config, &template_vars) {
            let mut msg = String::from("Template resolution errors:\n");
            for err in &errors {
                msg.push_str(&format!("  - {}\n", err));
            }
            bail!("{}", msg.trim_end());
        }

        // ================================================================
        // Phase 5: Spawn service supervisors
        // ================================================================
        let service_names: Vec<String> = launch_order
            .iter()
            .filter(|(_, k)| *k == ResourceKind::Service)
            .map(|(n, _)| n.clone())
            .collect();

        if !service_names.is_empty() {
            let max_name_len = launch_order.iter().map(|(n, _)| n.len()).max().unwrap_or(0);

            let (log_tx, log_rx) = mpsc::channel::<LogLine>(1024);
            let log_writer = LogWriter::new(log_rx, max_name_len);
            self.tracker.spawn(async move {
                log_writer.run().await;
            });

            for name in &service_names {
                let svc = &self.config.services[name];

                // Build env using the discovery module (global + DEVRIG_* + service overrides)
                let mut env = build_service_env(name, &self.config, &resolved_ports);

                // Add compose service discovery vars (build_service_env doesn't handle compose)
                for (cs_name, cs_state) in &compose_states {
                    let upper = cs_name.to_uppercase();
                    env.insert(format!("DEVRIG_{}_HOST", upper), "localhost".to_string());
                    if let Some(port) = cs_state.port {
                        env.insert(format!("DEVRIG_{}_PORT", upper), port.to_string());
                        env.insert(
                            format!("DEVRIG_{}_URL", upper),
                            format!("http://localhost:{}", port),
                        );
                    }
                }

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
                            if !e.to_string().contains("cancelled") {
                                error!(service = %svc_name, error = %e, "supervisor failed");
                            }
                        }
                    }
                });
            }

            // Drop our copy so LogWriter can detect when all supervisors are done
            drop(log_tx);
        }

        // ================================================================
        // Save state and register
        // ================================================================
        let mut service_states: BTreeMap<String, ServiceState> = BTreeMap::new();
        for name in &service_names {
            let svc = &self.config.services[name];
            let port = resolved_ports.get(&format!("service:{}", name)).copied();
            let port_auto = matches!(&svc.port, Some(Port::Auto));
            service_states.insert(
                name.clone(),
                ServiceState {
                    pid: 0,
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
            infra: infra_states.clone(),
            compose_services: compose_states.clone(),
            network_name: network_name.clone(),
        };
        project_state
            .save(&self.state_dir)
            .context("saving project state")?;

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

        // ================================================================
        // Print startup summary
        // ================================================================
        let mut summary_services: BTreeMap<String, RunningService> = BTreeMap::new();

        for (name, state) in &project_state.infra {
            summary_services.insert(
                format!("[infra] {}", name),
                RunningService {
                    port: state.port,
                    port_auto: state.port_auto,
                    status: "running".to_string(),
                },
            );
        }

        for (name, state) in &project_state.compose_services {
            summary_services.insert(
                format!("[compose] {}", name),
                RunningService {
                    port: state.port,
                    port_auto: false,
                    status: "running".to_string(),
                },
            );
        }

        for name in &service_names {
            let svc = &self.config.services[name];
            let port = resolved_ports.get(&format!("service:{}", name)).copied();
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

        // ================================================================
        // Wait for shutdown signal or all tasks to exit
        // ================================================================
        if service_names.is_empty() {
            // No services to supervise (infra/compose only) — wait for Ctrl+C
            tokio::signal::ctrl_c().await.ok();
            eprintln!("\nShutting down...");
        } else {
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
        }

        // Graceful shutdown: cancel supervisors
        self.cancel.cancel();
        self.tracker.close();
        match tokio::time::timeout(std::time::Duration::from_secs(10), self.tracker.wait()).await {
            Ok(()) => info!("All services stopped cleanly"),
            Err(_) => warn!("Shutdown timed out -- some processes may have been force-killed"),
        }

        // Stop infra containers on shutdown (preserve state for restart)
        for (name, infra_state) in &infra_states {
            if let Some(mgr) = &infra_mgr {
                if let Err(e) = mgr.stop_service(infra_state).await {
                    warn!(infra = %name, error = %e, "failed to stop infra container");
                }
            }
        }

        Ok(())
    }

    /// Stop a running project: cancel supervisors, stop infra containers.
    /// Preserves state and registry so `devrig ps` still sees it.
    pub async fn stop(&self) -> Result<()> {
        let state = ProjectState::load(&self.state_dir).ok_or_else(|| {
            anyhow::anyhow!("no running project state found -- is the project running?")
        })?;

        // Cancel service supervisors
        self.cancel.cancel();
        self.tracker.close();
        match tokio::time::timeout(std::time::Duration::from_secs(10), self.tracker.wait()).await {
            Ok(()) => info!("All services stopped cleanly"),
            Err(_) => warn!("Shutdown timed out -- some processes may have been force-killed"),
        }

        // Stop infra containers (preserve volumes/data)
        if !state.infra.is_empty() {
            match InfraManager::new(state.slug.clone()).await {
                Ok(mgr) => {
                    for (name, infra_state) in &state.infra {
                        if let Err(e) = mgr.stop_service(infra_state).await {
                            warn!(infra = %name, error = %e, "failed to stop infra container");
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "could not connect to Docker to stop infra containers");
                }
            }
        }

        Ok(())
    }

    /// Stop the project, remove all Docker resources, and unregister.
    pub async fn delete(&self) -> Result<()> {
        // Stop first (ignore errors if nothing is running)
        let _ = self.stop().await;

        // Clean up Docker resources (containers, volumes, networks)
        let state = ProjectState::load(&self.state_dir);
        if state
            .as_ref()
            .is_some_and(|s| !s.infra.is_empty() || s.network_name.is_some())
        {
            match InfraManager::new(self.identity.slug.clone()).await {
                Ok(mgr) => {
                    if let Err(e) = mgr.cleanup_all().await {
                        warn!(error = %e, "failed to clean up Docker resources");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "could not connect to Docker for cleanup");
                }
            }
        }

        // Compose down
        if let Some(compose_config) = &self.config.compose {
            let compose_file = self
                .config_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join(&compose_config.file);
            if let Err(e) =
                compose::lifecycle::compose_down(&compose_file, &self.identity.slug).await
            {
                warn!(error = %e, "failed to run compose down");
            }
        }

        // Remove project state file
        if let Err(e) = ProjectState::remove(&self.state_dir) {
            warn!(error = %e, "failed to remove project state");
        }

        // Remove the state directory entirely
        if self.state_dir.exists() {
            std::fs::remove_dir_all(&self.state_dir).with_context(|| {
                format!("removing state directory {}", self.state_dir.display())
            })?;
        }

        // Unregister from global registry
        let mut registry = InstanceRegistry::load();
        registry.unregister(&self.identity.slug);
        if let Err(e) = registry.save() {
            warn!(error = %e, "failed to save instance registry after unregister");
        }

        Ok(())
    }
}
