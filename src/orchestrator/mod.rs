pub mod graph;
pub mod ports;
pub mod registry;
pub mod state;
pub mod supervisor;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, error, warn};

use crate::cluster::addon::PortForwardManager;
use crate::cluster::K3dManager;
use crate::compose;
use crate::config;
use crate::config::interpolate::{build_template_vars, resolve_config_templates};
use crate::config::model::{DevrigConfig, Port};
use crate::config::validate::validate;
use crate::discovery::env::build_service_env;
use crate::identity::ProjectIdentity;
use crate::docker::DockerManager;
use crate::ui::logs::{LogLine, LogWriter};
use crate::ui::summary::{print_startup_banner, print_startup_summary, RunningService, StartupBannerInfo};

use graph::{DependencyResolver, ResourceKind};
use ports::{check_all_ports_unified, check_port_available, find_free_port_excluding, format_port_conflicts, resolve_port};
use registry::{InstanceEntry, InstanceRegistry};
use state::{
    ClusterDeployState, ClusterState, ComposeServiceState, DockerState, ProjectState, ServiceState,
};
use supervisor::{RestartPolicy, ServiceSupervisor};

/// Resolve a dashboard/OTel port: use the configured port if available,
/// otherwise auto-assign a free one. Tracks in `allocated` to avoid collisions.
fn resolve_dashboard_port(port_config: &Port, label: &str, allocated: &mut HashSet<u16>) -> u16 {
    match port_config {
        Port::Fixed(preferred) => {
            if !allocated.contains(preferred) && check_port_available(*preferred) {
                allocated.insert(*preferred);
                *preferred
            } else {
                let port = find_free_port_excluding(allocated);
                warn!("{label}: port {preferred} in use, using {port} instead");
                allocated.insert(port);
                port
            }
        }
        Port::Auto => {
            let port = find_free_port_excluding(allocated);
            allocated.insert(port);
            port
        }
    }
}

/// Central orchestrator that loads configuration, resolves dependencies,
/// manages Docker infrastructure, spawns supervised services, and handles
/// graceful shutdown.
///
/// Multi-phase startup order:
///   Phase 0 — Parse config, validate, load previous state
///   Phase 1 — Create Docker network (if docker/compose/cluster present)
///   Phase 2 — Compose up, bridge to network, ready checks
///   Phase 3 — Start docker containers in dependency order
///   Phase 3.5 — Create k3d cluster, deploy to cluster, start watchers
///   Phase 4 — Resolve ports, templates, DEVRIG_* env vars
///   Phase 5 — Spawn service supervisors with injected env
pub struct Orchestrator {
    config: DevrigConfig,
    identity: ProjectIdentity,
    config_path: PathBuf,
    state_dir: PathBuf,
    cancel: CancellationToken,
    tracker: TaskTracker,
    port_forward_mgr: Option<PortForwardManager>,
}

impl Orchestrator {
    /// Create an Orchestrator from a config file path.
    ///
    /// Loads and parses the config, validates it, and computes the project
    /// identity and state directory. Performs .env loading and $VAR expansion.
    pub fn from_config(config_path: PathBuf) -> Result<Self> {
        // Canonicalize so the dashboard config API (and state_dir) always resolve
        // correctly regardless of working-directory changes.
        let config_path = config_path
            .canonicalize()
            .with_context(|| format!("canonicalizing config path {}", config_path.display()))?;
        let (config, source, _secret_registry) = config::load_config_with_secrets(&config_path)
            .with_context(|| format!("loading config from {}", config_path.display()))?;

        let filename = config_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "devrig.toml".to_string());

        if let Err(errors) = validate(&config, &source, &filename) {
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
            port_forward_mgr: None,
        })
    }

    /// Start services according to the configuration.
    ///
    /// If `service_filter` is non-empty, only the named services (plus their
    /// transitive dependencies including docker/compose) are started.
    pub async fn start(&mut self, service_filter: Vec<String>, dev_mode: bool) -> Result<()> {
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
                    if let Some(docker_cfg) = self.config.docker.get(name) {
                        for dep in &docker_cfg.depends_on {
                            if needed.insert(dep.clone()) {
                                changed = true;
                            }
                        }
                    }
                    if let Some(cluster) = &self.config.cluster {
                        if let Some(image_cfg) = cluster.images.get(name) {
                            for dep in &image_cfg.depends_on {
                                if needed.insert(dep.clone()) {
                                    changed = true;
                                }
                            }
                        }
                        if let Some(deploy) = cluster.deploy.get(name) {
                            for dep in &deploy.depends_on {
                                if needed.insert(dep.clone()) {
                                    changed = true;
                                }
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

        let dashboard_enabled = self
            .config
            .dashboard
            .as_ref()
            .is_some_and(|d| d.enabled.unwrap_or(true));

        if launch_order.is_empty() && !dashboard_enabled {
            bail!("no resources to start");
        }

        // Check port conflicts for all fixed ports (services + docker)
        let conflicts = check_all_ports_unified(&self.config);
        if !conflicts.is_empty() {
            bail!("{}", format_port_conflicts(&conflicts));
        }

        // Create state directory
        std::fs::create_dir_all(&self.state_dir)
            .with_context(|| format!("creating state dir {}", self.state_dir.display()))?;

        // Write PID file so `devrig stop` can signal this process
        let pid_path = self.state_dir.join("pid");
        std::fs::write(&pid_path, std::process::id().to_string())
            .with_context(|| format!("writing PID file {}", pid_path.display()))?;

        // Print startup banner
        {
            let banner_services: Vec<String> = launch_order
                .iter()
                .filter(|(_, k)| matches!(k, ResourceKind::Service))
                .map(|(n, _)| n.clone())
                .collect();
            let banner_docker: Vec<String> = launch_order
                .iter()
                .filter(|(_, k)| matches!(k, ResourceKind::Docker))
                .map(|(n, _)| n.clone())
                .collect();
            let banner_compose = self.config.compose.as_ref().map(|c| {
                c.file.clone()
            });
            let banner_addons: Vec<String> = self
                .config
                .cluster
                .as_ref()
                .map(|c| c.addons.keys().cloned().collect())
                .unwrap_or_default();

            let info = StartupBannerInfo {
                services: banner_services,
                docker: banner_docker,
                compose: banner_compose,
                cluster_addons: banner_addons,
                dashboard_enabled,
            };
            print_startup_banner(&self.identity, &info);
        }

        let has_docker = launch_order.iter().any(|(_, k)| {
            matches!(
                k,
                ResourceKind::Docker
                    | ResourceKind::Compose
                    | ResourceKind::ClusterImage
                    | ResourceKind::ClusterDeploy
            )
        }) || self.config.cluster.is_some();

        // ================================================================
        // Phase 0.5: Dashboard + OTel collector (start early to capture all telemetry)
        // ================================================================
        let mut allocated_ports: HashSet<u16> = HashSet::new();
        let mut dashboard_state: Option<state::DashboardState> = None;
        let mut _otel_collector: Option<crate::otel::OtelCollector> = None;
        // Clones of store/events for the log bridge (used later for docker + service logs)
        let mut bridge_store: Option<Arc<tokio::sync::RwLock<crate::otel::storage::TelemetryStore>>> = None;
        let mut bridge_events_tx: Option<broadcast::Sender<crate::otel::types::TelemetryEvent>> = None;

        if dashboard_enabled {
            let dash_config = self.config.dashboard.as_ref().unwrap();
            let otel_config = dash_config.otel.clone().unwrap_or_default();

            // Auto-resolve dashboard/OTel ports: use configured port if free,
            // otherwise find an available one. This lets multiple devrig instances
            // run without port conflicts.
            let dash_port = resolve_dashboard_port(&dash_config.port, "dashboard", &mut allocated_ports);
            let otel_grpc = resolve_dashboard_port(&otel_config.grpc_port, "otel-grpc", &mut allocated_ports);
            let otel_http = resolve_dashboard_port(&otel_config.http_port, "otel-http", &mut allocated_ports);

            // Use resolved ports for the collector
            let mut resolved_otel = otel_config;
            resolved_otel.grpc_port = Port::Fixed(otel_grpc);
            resolved_otel.http_port = Port::Fixed(otel_http);

            let collector = crate::otel::OtelCollector::new(&resolved_otel);
            collector
                .start(self.cancel.clone())
                .await
                .context("starting OTel collector")?;
            debug!(
                grpc_port = otel_grpc,
                http_port = otel_http,
                "OTel collector started"
            );

            let store = collector.store();
            let events_tx = collector.events_tx();

            // Clone for the log bridge
            bridge_store = Some(Arc::clone(&store));
            bridge_events_tx = Some(events_tx.clone());

            let dash_cancel = self.cancel.clone();
            let dash_config_path = Some(self.config_path.clone());
            let dash_state_dir = Some(self.state_dir.clone());
            self.tracker.spawn(async move {
                if let Err(e) = crate::dashboard::server::start_dashboard_server(
                    dash_port,
                    store,
                    events_tx,
                    dash_cancel,
                    dash_config_path,
                    dash_state_dir,
                )
                .await
                {
                    warn!(error = %e, "Dashboard server failed");
                }
            });
            debug!(port = dash_port, "Dashboard server started");

            dashboard_state = Some(state::DashboardState {
                dashboard_port: dash_port,
                grpc_port: otel_grpc,
                http_port: otel_http,
            });
            _otel_collector = Some(collector);
        }

        // ================================================================
        // Phase 1: Docker network
        // ================================================================
        let docker_mgr = if has_docker {
            let mgr = DockerManager::new(self.identity.slug.clone()).await?;
            mgr.ensure_network().await?;
            debug!(network = %mgr.network_name(), "Docker network ensured");
            Some(mgr)
        } else {
            None
        };

        let network_name = docker_mgr.as_ref().map(|m| m.network_name());

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
                debug!(services = ?compose_services, "starting compose services");
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
                if let Some(mgr) = &docker_mgr {
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

                debug!(count = compose_states.len(), "compose services started");
            }
        }

        // ================================================================
        // Phase 3: Infrastructure containers (in dependency order)
        // ================================================================
        let config_dir = self
            .config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let mut docker_states: BTreeMap<String, DockerState> = BTreeMap::new();

        // Pre-populate allocated ports from compose services
        for cs in compose_states.values() {
            if let Some(port) = cs.port {
                allocated_ports.insert(port);
            }
        }

        for (name, kind) in &launch_order {
            if *kind != ResourceKind::Docker {
                continue;
            }

            let docker_config = self
                .config
                .docker
                .get(name)
                .ok_or_else(|| anyhow::anyhow!("docker '{}' not found in config", name))?
                .clone();

            let prev_docker = prev_state.as_ref().and_then(|s| s.docker.get(name));

            debug!(docker = %name, image = %docker_config.image, "starting docker service");

            let state = docker_mgr
                .as_ref()
                .expect("docker_mgr must exist when docker resources are present")
                .start_service(name, &docker_config, prev_docker, &mut allocated_ports, &config_dir)
                .await
                .with_context(|| format!("starting docker service '{}'", name))?;

            docker_states.insert(name.clone(), state);
        }

        // ================================================================
        // Phase 3.5: k3d Cluster
        // ================================================================
        let mut cluster_state: Option<ClusterState> = None;

        if let Some(cluster_config) = &self.config.cluster {
            let network = network_name
                .as_deref()
                .expect("network must exist when cluster is configured");

            let k3d_mgr = K3dManager::new(
                &self.identity.slug,
                cluster_config,
                &self.state_dir,
                network,
                &config_dir,
            );

            debug!(cluster = %k3d_mgr.cluster_name(), "creating k3d cluster");
            k3d_mgr
                .create_cluster()
                .await
                .context("creating k3d cluster")?;
            k3d_mgr
                .write_kubeconfig()
                .await
                .context("writing kubeconfig")?;
            debug!(
                kubeconfig = %k3d_mgr.kubeconfig_path().display(),
                "kubeconfig written"
            );

            // Discover registry port if registry is enabled
            let registry_port = if cluster_config.registry {
                let port = crate::cluster::registry::get_registry_port(&self.identity.slug)
                    .await
                    .context("discovering registry port")?;
                crate::cluster::registry::wait_for_registry(port)
                    .await
                    .context("waiting for registry")?;
                debug!(port = port, "local registry ready");
                Some(port)
            } else {
                None
            };

            // Build and push cluster images in dependency order

            let mut deployed: BTreeMap<String, ClusterDeployState> = BTreeMap::new();

            for (name, kind) in &launch_order {
                if *kind != ResourceKind::ClusterImage {
                    continue;
                }

                let image_config = cluster_config
                    .images
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("cluster image '{}' not in config", name))?;

                debug!(image = %name, "building cluster image");
                let state = crate::cluster::deploy::run_image_build(
                    name,
                    image_config,
                    registry_port,
                    &config_dir,
                    &deployed,
                    &self.cancel,
                )
                .await
                .with_context(|| format!("building cluster image '{}'", name))?;

                deployed.insert(name.clone(), state);
            }

            // Deploy cluster services in dependency order
            for (name, kind) in &launch_order {
                if *kind != ResourceKind::ClusterDeploy {
                    continue;
                }

                let deploy_config = cluster_config
                    .deploy
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("cluster deploy '{}' not in config", name))?;

                debug!(deploy = %name, "deploying to cluster");
                let state = crate::cluster::deploy::run_deploy(
                    name,
                    deploy_config,
                    registry_port,
                    k3d_mgr.kubeconfig_path(),
                    &config_dir,
                    &self.cancel,
                )
                .await
                .with_context(|| format!("deploying '{}' to cluster", name))?;

                deployed.insert(name.clone(), state);
            }

            // Start file watchers for watch=true deploys
            crate::cluster::watcher::start_watchers(
                &cluster_config.deploy,
                registry_port,
                k3d_mgr.kubeconfig_path().to_path_buf(),
                config_dir.clone(),
                self.cancel.clone(),
                &self.tracker,
            )
            .await
            .context("starting file watchers")?;

            // Start file watchers for watch=true images
            crate::cluster::watcher::start_image_watchers(
                &cluster_config.images,
                registry_port,
                config_dir.clone(),
                deployed.clone(),
                self.cancel.clone(),
                &self.tracker,
            )
            .await
            .context("starting image file watchers")?;

            // Inject synthetic Fluent Bit log collector addon if configured
            let mut combined_addons = cluster_config.addons.clone();
            if let Some(logs_config) = &cluster_config.logs {
                if logs_config.enabled && logs_config.collector {
                    let otel_http_port = dashboard_state.as_ref()
                        .map(|ds| ds.http_port)
                        .unwrap_or(4318);
                    let otlp_endpoint = format!("host.k3d.internal:{}", otel_http_port);
                    let manifest_content = crate::cluster::log_collector::render_fluent_bit_manifest(
                        logs_config,
                        &otlp_endpoint,
                    );
                    let manifest_path = self.state_dir.join(
                        crate::cluster::log_collector::MANIFEST_FILENAME,
                    );
                    std::fs::write(&manifest_path, &manifest_content)
                        .with_context(|| format!(
                            "writing Fluent Bit manifest to {}",
                            manifest_path.display()
                        ))?;

                    combined_addons.insert(
                        crate::cluster::log_collector::ADDON_KEY.to_string(),
                        crate::config::model::AddonConfig::Manifest {
                            path: manifest_path.to_string_lossy().to_string(),
                            namespace: None,
                            port_forward: BTreeMap::new(),
                            depends_on: vec![],
                        },
                    );
                    debug!("Fluent Bit log collector manifest generated");
                }
            }

            // Install addons (helm charts, manifests, kustomize)
            let installed_addons = if !combined_addons.is_empty() {
                debug!(
                    count = combined_addons.len(),
                    "installing cluster addons"
                );

                // Build template vars available at addon-install time:
                // cluster images, registry, project name, and any ports
                // already resolved (dashboard, docker, compose, fixed
                // service ports).
                let mut addon_template_vars =
                    crate::config::interpolate::build_cluster_image_vars(&deployed);
                if cluster_config.registry {
                    addon_template_vars.insert(
                        "cluster.registry".to_string(),
                        format!("k3d-devrig-{}-reg:5000", self.identity.slug),
                    );
                }

                addon_template_vars.insert(
                    "project.name".to_string(),
                    self.config.project.name.clone(),
                );

                // Fixed service ports (available before service launch)
                for (name, svc) in &self.config.services {
                    if let Some(crate::config::model::Port::Fixed(port)) = &svc.port {
                        addon_template_vars
                            .insert(format!("services.{name}.port"), port.to_string());
                    }
                }

                // Dashboard / OTel ports
                if let Some(ref ds) = dashboard_state {
                    addon_template_vars
                        .insert("dashboard.port".to_string(), ds.dashboard_port.to_string());
                    addon_template_vars.insert(
                        "dashboard.otel.grpc_port".to_string(),
                        ds.grpc_port.to_string(),
                    );
                    addon_template_vars.insert(
                        "dashboard.otel.http_port".to_string(),
                        ds.http_port.to_string(),
                    );
                }

                // Docker ports
                for (name, state) in &docker_states {
                    if let Some(port) = state.port {
                        addon_template_vars
                            .insert(format!("docker.{name}.port"), port.to_string());
                    }
                    for (pname, &port) in &state.named_ports {
                        let val = port.to_string();
                        addon_template_vars
                            .insert(format!("docker.{name}.ports.{pname}"), val.clone());
                        addon_template_vars
                            .insert(format!("docker.{name}.port_{pname}"), val);
                    }
                }

                // Compose ports
                for (name, state) in &compose_states {
                    if let Some(port) = state.port {
                        addon_template_vars
                            .insert(format!("compose.{name}.port"), port.to_string());
                    }
                }

                crate::cluster::addon::install_addons(
                    &combined_addons,
                    &addon_template_vars,
                    k3d_mgr.kubeconfig_path(),
                    &config_dir,
                    &self.cancel,
                )
                .await
                .context("installing cluster addons")?
            } else {
                BTreeMap::new()
            };

            // Start port-forwards for addons
            let pf_mgr = PortForwardManager::new();
            pf_mgr.start_port_forwards(&cluster_config.addons, k3d_mgr.kubeconfig_path());
            self.port_forward_mgr = Some(pf_mgr);

            let registry_name = if cluster_config.registry {
                Some(format!("k3d-devrig-{}-reg", self.identity.slug))
            } else {
                None
            };

            cluster_state = Some(ClusterState {
                cluster_name: k3d_mgr.cluster_name().to_string(),
                kubeconfig_path: k3d_mgr.kubeconfig_path().to_string_lossy().to_string(),
                registry_name,
                registry_port,
                deployed_services: deployed,
                installed_addons,
            });
        }

        // ================================================================
        // Phase 4: Resolve ports, templates, and env vars
        // ================================================================
        let mut resolved_ports: HashMap<String, u16> = HashMap::new();

        // Dashboard/OTel resolved ports (for template interpolation)
        if let Some(ref ds) = dashboard_state {
            resolved_ports.insert("dashboard".to_string(), ds.dashboard_port);
            resolved_ports.insert("otel-grpc".to_string(), ds.grpc_port);
            resolved_ports.insert("otel-http".to_string(), ds.http_port);
        }

        // Docker ports
        for (name, state) in &docker_states {
            if let Some(port) = state.port {
                resolved_ports.insert(format!("docker:{}", name), port);
            }
            for (pname, &port) in &state.named_ports {
                resolved_ports.insert(format!("docker:{}:{}", name, pname), port);
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

        // Merge cluster vars into service template vars
        if let Some(ref cs) = cluster_state {
            let image_vars =
                crate::config::interpolate::build_cluster_image_vars(&cs.deployed_services);
            template_vars.extend(image_vars);
            template_vars.insert(
                "cluster.kubeconfig".to_string(),
                cs.kubeconfig_path.clone(),
            );
        }

        if let Err(errors) = resolve_config_templates(&mut self.config, &template_vars) {
            let mut msg = String::from("Template resolution errors:\n");
            for err in &errors {
                msg.push_str(&format!("  - {}\n", err));
            }
            bail!("{}", msg.trim_end());
        }

        // ================================================================
        // Phase 4.55: Docker container log streams → dashboard
        // ================================================================
        if let (Some(ref b_store), Some(ref b_events)) = (&bridge_store, &bridge_events_tx) {
            if let Some(ref mgr) = docker_mgr {
                for (name, state) in &docker_states {
                    crate::docker::log_stream::spawn_docker_log_stream(
                        mgr.docker().clone(),
                        state.container_id.clone(),
                        name.clone(),
                        Arc::clone(b_store),
                        b_events.clone(),
                        self.cancel.clone(),
                        &self.tracker,
                    );
                }
                for (name, state) in &compose_states {
                    crate::docker::log_stream::spawn_docker_log_stream(
                        mgr.docker().clone(),
                        state.container_id.clone(),
                        name.clone(),
                        Arc::clone(b_store),
                        b_events.clone(),
                        self.cancel.clone(),
                        &self.tracker,
                    );
                }
            }
        }

        // ================================================================
        // Phase 4.6: Vite dev server (--dev mode)
        // ================================================================
        let mut vite_port: Option<u16> = None;

        if dev_mode {
            let dashboard_dir = self
                .config_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join("dashboard");

            if !dashboard_dir.join("package.json").exists() {
                bail!(
                    "--dev requires dashboard/ directory with package.json (at {})",
                    dashboard_dir.display()
                );
            }

            let cancel = self.cancel.clone();
            self.tracker.spawn(async move {
                let mut child = tokio::process::Command::new("bun")
                    .args(["run", "dev"])
                    .current_dir(&dashboard_dir)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                    .expect("failed to spawn Vite dev server (is bun installed?)");

                tokio::select! {
                    _ = cancel.cancelled() => {
                        let _ = child.kill().await;
                    }
                    status = child.wait() => {
                        if let Ok(s) = status {
                            if !s.success() {
                                warn!(code = ?s.code(), "Vite dev server exited");
                            }
                        }
                    }
                }
            });

            vite_port = Some(5173);
            debug!(port = 5173, "Vite dev server started");
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

            // Supervisors send to log_tx (broadcast). A fan-out task distributes
            // to the terminal writer and the JSONL file writer.
            let (log_tx, _) = broadcast::channel::<LogLine>(4096);
            let (display_tx, display_rx) = mpsc::channel::<LogLine>(1024);

            let log_writer = LogWriter::new(display_rx, max_name_len);
            self.tracker.spawn(async move {
                log_writer.run().await;
            });

            // JSONL log file writer
            let logs_dir = self.state_dir.join("logs");
            let _ = std::fs::create_dir_all(&logs_dir);
            let jsonl_path = logs_dir.join("current.jsonl");
            let jsonl_file = std::fs::File::create(&jsonl_path).ok();

            // Fan-out task: subscribes to broadcast, forwards to display + JSONL
            let mut fan_rx = log_tx.subscribe();
            self.tracker.spawn(async move {
                let mut jsonl_writer = jsonl_file.map(std::io::BufWriter::new);
                loop {
                    match fan_rx.recv().await {
                        Ok(line) => {
                            // Write to JSONL file
                            if let Some(ref mut w) = jsonl_writer {
                                use std::io::Write;
                                if let Ok(json) = serde_json::to_string(&line) {
                                    let _ = writeln!(w, "{}", json);
                                    let _ = w.flush();
                                }
                            }
                            // Send to terminal display
                            let _ = display_tx.send(line).await;
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    }
                }
            });

            // Log bridge: forwards supervisor LogLine → TelemetryStore so
            // process stdout/stderr appears in the dashboard Logs view.
            if let (Some(b_store), Some(b_events)) = (bridge_store.clone(), bridge_events_tx.clone()) {
                let mut bridge_rx = log_tx.subscribe();
                self.tracker.spawn(async move {
                    loop {
                        match bridge_rx.recv().await {
                            Ok(line) => {
                                let stored = crate::otel::types::logline_to_stored(&line);
                                let event = crate::otel::types::TelemetryEvent::LogRecord {
                                    trace_id: None,
                                    severity: format!("{:?}", stored.severity),
                                    body: stored.body.clone(),
                                    service: stored.service_name.clone(),
                                };
                                { b_store.write().await.insert_log(stored); }
                                let _ = b_events.send(event);
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!(skipped = n, "log bridge lagged");
                                continue;
                            }
                        }
                    }
                });
            }

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

                // Inject OTel env vars with resolved ports (overrides build_service_env defaults)
                if let Some(ref ds) = dashboard_state {
                    env.insert(
                        "OTEL_EXPORTER_OTLP_ENDPOINT".to_string(),
                        format!("http://localhost:{}", ds.http_port),
                    );
                    env.insert("OTEL_SERVICE_NAME".to_string(), name.clone());
                    env.insert(
                        "DEVRIG_DASHBOARD_URL".to_string(),
                        format!("http://localhost:{}", ds.dashboard_port),
                    );
                }

                let working_dir = svc.path.as_ref().map(|p| {
                    let base = self
                        .config_path
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new("."));
                    base.join(p)
                });

                let policy = match &svc.restart {
                    Some(cfg) => RestartPolicy::from_config(cfg),
                    None => RestartPolicy::default(),
                };

                let supervisor = ServiceSupervisor::new(
                    name.clone(),
                    svc.command.clone(),
                    working_dir,
                    env,
                    policy,
                    log_tx.clone(),
                    self.cancel.clone(),
                );

                let svc_name = name.clone();
                let state_dir_clone = self.state_dir.clone();
                self.tracker.spawn(async move {
                    let (phase, exit_code) = match supervisor.run().await {
                        Ok(status) => {
                            debug!(service = %svc_name, %status, "supervisor finished");
                            let code = status.code();
                            let phase = if code == Some(0) { "stopped" } else { "failed" };
                            (phase.to_string(), code)
                        }
                        Err(e) => {
                            if !e.to_string().contains("cancelled") {
                                error!(service = %svc_name, error = %e, "supervisor failed");
                                ("failed".to_string(), None)
                            } else {
                                ("stopped".to_string(), None)
                            }
                        }
                    };

                    // Update state.json with exit info
                    if let Some(mut state) = ProjectState::load(&state_dir_clone) {
                        if let Some(svc_state) = state.services.get_mut(&svc_name) {
                            svc_state.phase = Some(phase);
                            svc_state.exit_code = exit_code;
                        }
                        if let Err(e) = state.save(&state_dir_clone) {
                            warn!(service = %svc_name, error = %e, "failed to update service exit state");
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
                    protocol: svc.protocol.clone(),
                    phase: Some("starting".to_string()),
                    exit_code: None,
                },
            );
        }

        let project_state = ProjectState {
            slug: self.identity.slug.clone(),
            config_path: self.config_path.to_string_lossy().to_string(),
            services: service_states,
            started_at: Utc::now(),
            docker: docker_states.clone(),
            compose_services: compose_states.clone(),
            network_name: network_name.clone(),
            cluster: cluster_state.clone(),
            dashboard: dashboard_state.clone(),
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

        for (name, state) in &project_state.docker {
            summary_services.insert(
                format!("[docker] {}", name),
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

        if let Some(cs) = &cluster_state {
            for (name, deploy_state) in &cs.deployed_services {
                // Check if this is a cluster image (not a deploy)
                let is_image = self
                    .config
                    .cluster
                    .as_ref()
                    .is_some_and(|c| c.images.contains_key(name));

                if is_image {
                    let watch_tag = self
                        .config
                        .cluster
                        .as_ref()
                        .and_then(|c| c.images.get(name))
                        .map(|i| i.watch)
                        .unwrap_or(false);
                    let status = if watch_tag {
                        "built (watching)".to_string()
                    } else {
                        "built".to_string()
                    };
                    summary_services.insert(
                        format!("[image] {}", name),
                        RunningService {
                            port: None,
                            port_auto: false,
                            status: format!("{} [{}]", status, deploy_state.image_tag),
                        },
                    );
                } else {
                    let watch_tag = self
                        .config
                        .cluster
                        .as_ref()
                        .and_then(|c| c.deploy.get(name))
                        .map(|d| d.watch)
                        .unwrap_or(false);
                    let status = if watch_tag {
                        "deployed (watching)".to_string()
                    } else {
                        "deployed".to_string()
                    };
                    summary_services.insert(
                        format!("[cluster] {}", name),
                        RunningService {
                            port: None,
                            port_auto: false,
                            status: format!("{} [{}]", status, deploy_state.image_tag),
                        },
                    );
                }
            }

            // Addon summary entries
            for (name, addon_state) in &cs.installed_addons {
                let pf_port = self
                    .config
                    .cluster
                    .as_ref()
                    .and_then(|c| c.addons.get(name))
                    .and_then(|a| {
                        a.port_forward()
                            .keys()
                            .next()
                            .and_then(|p| p.parse::<u16>().ok())
                    });
                summary_services.insert(
                    format!("[addon] {}", name),
                    RunningService {
                        port: pf_port,
                        port_auto: false,
                        status: format!("installed ({})", addon_state.addon_type),
                    },
                );
            }
        }

        if let Some(ref ds) = dashboard_state {
            let dash_config = self.config.dashboard.as_ref().unwrap();
            let otel_config = dash_config.otel.clone().unwrap_or_default();
            summary_services.insert(
                "[dashboard]".to_string(),
                RunningService {
                    port: Some(ds.dashboard_port),
                    port_auto: dash_config.port.is_auto(),
                    status: "running".to_string(),
                },
            );
            summary_services.insert(
                "[otel] grpc".to_string(),
                RunningService {
                    port: Some(ds.grpc_port),
                    port_auto: otel_config.grpc_port.is_auto(),
                    status: "running".to_string(),
                },
            );
            summary_services.insert(
                "[otel] http".to_string(),
                RunningService {
                    port: Some(ds.http_port),
                    port_auto: otel_config.http_port.is_auto(),
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

        if let Some(port) = vite_port {
            summary_services.insert(
                "[vite]".to_string(),
                RunningService {
                    port: Some(port),
                    port_auto: false,
                    status: "running".to_string(),
                },
            );
        }

        print_startup_summary(&self.identity, &summary_services);

        // ================================================================
        // Wait for shutdown signal (SIGINT/SIGTERM) or all tasks to exit
        // ================================================================
        let wait_for_signal = async {
            #[cfg(unix)]
            {
                let mut sigterm =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("failed to register SIGTERM handler");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = sigterm.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                tokio::signal::ctrl_c().await.ok();
            }
        };

        if service_names.is_empty() {
            wait_for_signal.await;
            eprintln!("\nShutting down...");
        } else {
            tokio::select! {
                _ = wait_for_signal => {
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

        // Graceful shutdown: cancel supervisors, with second Ctrl+C for force exit
        self.cancel.cancel();
        self.tracker.close();
        let shutdown_fut = async {
            match tokio::time::timeout(std::time::Duration::from_secs(10), self.tracker.wait())
                .await
            {
                Ok(()) => debug!("All services stopped cleanly"),
                Err(_) => warn!("Shutdown timed out -- some processes may have been force-killed"),
            }

            // Stop addon port-forwards
            if let Some(pf_mgr) = &self.port_forward_mgr {
                pf_mgr.stop().await;
            }

            // Stop docker containers on shutdown (preserve state for restart)
            for (name, docker_state) in &docker_states {
                if let Some(mgr) = &docker_mgr {
                    if let Err(e) = mgr.stop_service(docker_state).await {
                        warn!(docker = %name, error = %e, "failed to stop docker container");
                    }
                }
            }
        };

        tokio::select! {
            () = shutdown_fut => {}
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nForce shutdown");
                let _ = std::fs::remove_file(self.state_dir.join("pid"));
                std::process::exit(130);
            }
        }

        // Clean up PID file
        let _ = std::fs::remove_file(self.state_dir.join("pid"));

        Ok(())
    }

    /// Stop a running project: signal the running devrig process via PID file,
    /// or stop docker containers directly.
    pub async fn stop(&self) -> Result<()> {
        let _state = ProjectState::load(&self.state_dir).ok_or_else(|| {
            anyhow::anyhow!("no running project state found -- is the project running?")
        })?;

        // Signal the running devrig process via PID file
        let pid_path = self.state_dir.join("pid");
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                #[cfg(unix)]
                {
                    use nix::sys::signal::{kill, Signal};
                    use nix::unistd::Pid;
                    match kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
                        Ok(()) => {
                            eprintln!("Sent stop signal to devrig (pid {pid})");
                            // Wait for the process to exit
                            for _ in 0..100 {
                                if kill(Pid::from_raw(pid as i32), None).is_err() {
                                    break; // Process exited
                                }
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            }
                            return Ok(());
                        }
                        Err(nix::errno::Errno::ESRCH) => {
                            // Process doesn't exist — stale PID file
                            let _ = std::fs::remove_file(&pid_path);
                            eprintln!("Removed stale PID file (process {pid} not found)");
                        }
                        Err(e) => {
                            warn!(pid, error = %e, "failed to signal devrig process");
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    eprintln!("Sent stop signal to devrig (pid {pid})");
                    // On non-Unix, fall through to docker cleanup
                }
            }
        } else {
            eprintln!("No PID file found — the project may have been started in a previous version.");
        }

        // Fallback: stop docker containers directly (preserve volumes/data)
        if !_state.docker.is_empty() {
            match DockerManager::new(_state.slug.clone()).await {
                Ok(mgr) => {
                    for (name, docker_state) in &_state.docker {
                        if let Err(e) = mgr.stop_service(docker_state).await {
                            warn!(docker = %name, error = %e, "failed to stop docker container");
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "could not connect to Docker to stop docker containers");
                }
            }
        }

        Ok(())
    }

    /// Stop the project, remove all Docker resources, and unregister.
    pub async fn delete(&self) -> Result<()> {
        // Stop first (ignore errors if nothing is running)
        let _ = self.stop().await;

        // Delete k3d cluster if it exists
        let state = ProjectState::load(&self.state_dir);
        if let Some(cs) = state.as_ref().and_then(|s| s.cluster.as_ref()) {
            if let Some(cluster_config) = &self.config.cluster {
                let network = state
                    .as_ref()
                    .and_then(|s| s.network_name.as_deref())
                    .unwrap_or("devrig-net");
                let delete_config_dir = self
                    .config_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."));
                let k3d_mgr = K3dManager::new(
                    &self.identity.slug,
                    cluster_config,
                    &self.state_dir,
                    network,
                    delete_config_dir,
                );

                // Uninstall addons (including synthetic log collector) before deleting the cluster
                let mut uninstall_addons = cluster_config.addons.clone();
                let log_collector_manifest = self.state_dir.join(
                    crate::cluster::log_collector::MANIFEST_FILENAME,
                );
                if log_collector_manifest.exists() {
                    uninstall_addons.insert(
                        crate::cluster::log_collector::ADDON_KEY.to_string(),
                        crate::config::model::AddonConfig::Manifest {
                            path: log_collector_manifest.to_string_lossy().to_string(),
                            namespace: None,
                            port_forward: BTreeMap::new(),
                            depends_on: vec![],
                        },
                    );
                }
                if !uninstall_addons.is_empty() {
                    debug!("uninstalling cluster addons before deletion");
                    let cancel = CancellationToken::new();
                    let config_dir = self
                        .config_path
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new("."))
                        .to_path_buf();
                    crate::cluster::addon::uninstall_addons(
                        &uninstall_addons,
                        k3d_mgr.kubeconfig_path(),
                        &config_dir,
                        &cancel,
                    )
                    .await;
                }

                debug!(cluster = %cs.cluster_name, "deleting k3d cluster");
                if let Err(e) = k3d_mgr.delete_cluster().await {
                    warn!(error = %e, "failed to delete k3d cluster");
                }
            }
        }

        // Clean up Docker resources (containers, volumes, networks)
        if state
            .as_ref()
            .is_some_and(|s| !s.docker.is_empty() || s.network_name.is_some())
        {
            match DockerManager::new(self.identity.slug.clone()).await {
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
