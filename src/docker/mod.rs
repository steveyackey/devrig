pub mod container;
pub mod exec;
pub mod image;
pub mod network;
pub mod ready;
pub mod volume;

use anyhow::{Context, Result};
use bollard::Docker;
use std::collections::HashSet;

use crate::config::model::{DockerConfig, Port};
use crate::docker::container::PortMap;
use crate::docker::network::resource_labels;
use crate::orchestrator::ports::resolve_port;
use crate::orchestrator::state::DockerState;

/// Manages Docker infrastructure containers for a devrig project.
pub struct DockerManager {
    docker: Docker,
    slug: String,
}

impl DockerManager {
    /// Create a new DockerManager, verifying Docker daemon connectivity.
    pub async fn new(slug: String) -> Result<Self> {
        let docker =
            Docker::connect_with_local_defaults().context("connecting to Docker daemon")?;
        docker
            .ping()
            .await
            .context("Cannot connect to Docker daemon. Is Docker running?")?;
        Ok(Self { docker, slug })
    }

    /// Get a reference to the Docker client.
    pub fn docker(&self) -> &Docker {
        &self.docker
    }

    /// Get the project slug.
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// Get the project network name.
    pub fn network_name(&self) -> String {
        format!("devrig-{}-net", self.slug)
    }

    /// Ensure the project Docker network exists.
    pub async fn ensure_network(&self) -> Result<()> {
        let network_name = self.network_name();
        let labels = resource_labels(&self.slug, "network");
        network::ensure_network(&self.docker, &network_name, labels).await
    }

    /// Start a single docker service: pull image, create volumes, create and
    /// start container, run ready check, run init scripts if needed.
    pub async fn start_service(
        &self,
        name: &str,
        config: &DockerConfig,
        prev_state: Option<&DockerState>,
        allocated_ports: &mut HashSet<u16>,
    ) -> Result<DockerState> {
        // Pull image if needed (with optional registry auth)
        if !image::check_image_exists(&self.docker, &config.image).await {
            image::pull_image_with_auth(&self.docker, &config.image, config.registry_auth.as_ref())
                .await?;
        }

        // Resolve ports
        let mut port: Option<u16> = None;
        let mut port_auto = false;
        let mut named_ports = std::collections::BTreeMap::new();

        if let Some(port_config) = &config.port {
            let prev_port = prev_state.and_then(|s| s.port);
            let prev_auto = prev_state.map(|s| s.port_auto).unwrap_or(false);
            let resolved = resolve_port(
                &format!("docker:{}", name),
                port_config,
                prev_port,
                prev_auto,
                allocated_ports,
            );
            port = Some(resolved);
            port_auto = port_config.is_auto();
        }

        for (port_name, port_config) in &config.ports {
            let prev_port = prev_state
                .and_then(|s| s.named_ports.get(port_name))
                .copied();
            let prev_auto = port_config.is_auto();
            let resolved = resolve_port(
                &format!("docker:{}:{}", name, port_name),
                port_config,
                prev_port,
                prev_auto,
                allocated_ports,
            );
            named_ports.insert(port_name.clone(), resolved);
        }

        // Create volumes
        let mut volume_binds = Vec::new();
        for vol_spec in &config.volumes {
            if let Some((vol_name, container_path)) =
                volume::parse_volume_spec(vol_spec, &self.slug)
            {
                let labels = resource_labels(&self.slug, name);
                volume::ensure_volume(&self.docker, &vol_name, labels).await?;
                volume_binds.push((vol_name, container_path));
            }
        }

        // Build port mappings
        let mut port_maps = Vec::new();
        if let Some(host_port) = port {
            // Use the container's default port (from image) — we need to know it.
            // For single-port services, the container port is the same as the default
            // port for the service type, or we use the host port.
            let container_port = match &config.port {
                Some(Port::Fixed(p)) => *p,
                _ => host_port,
            };
            port_maps.push(PortMap {
                container_port,
                host_port,
            });
        }
        for (port_name, port_config) in &config.ports {
            if let Some(&host_port) = named_ports.get(port_name) {
                let container_port = match port_config {
                    Port::Fixed(p) => *p,
                    Port::Auto => host_port,
                };
                port_maps.push(PortMap {
                    container_port,
                    host_port,
                });
            }
        }

        // Build env vars
        let env_vars: Vec<(String, String)> = config
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let network_name = self.network_name();

        // Create and start container
        let container_name = format!("devrig-{}-{}", self.slug, name);
        let container_id = container::create_container(
            &self.docker,
            &self.slug,
            name,
            &config.image,
            &env_vars,
            &port_maps,
            &volume_binds,
            &network_name,
        )
        .await?;

        container::start_container(&self.docker, &container_id).await?;
        tracing::info!(docker = %name, container = %container_name, "container started");

        // Run ready check
        if let Some(check) = &config.ready_check {
            tracing::info!(docker = %name, "waiting for ready check");
            ready::run_ready_check(&self.docker, &container_id, check, port, name).await?;
            tracing::info!(docker = %name, "ready");
        }

        // Run init scripts (only if not already completed)
        let already_init = prev_state.map(|s| s.init_completed).unwrap_or(false);
        let mut init_completed = already_init;
        let mut init_completed_at = prev_state.and_then(|s| s.init_completed_at);

        if !already_init && !config.init.is_empty() {
            exec::run_init_scripts(&self.docker, &container_id, name, config).await?;
            init_completed = true;
            init_completed_at = Some(chrono::Utc::now());
            tracing::info!(docker = %name, "init scripts completed");
        }

        Ok(DockerState {
            container_id,
            container_name,
            port,
            port_auto,
            named_ports,
            init_completed,
            init_completed_at,
        })
    }

    /// Stop a single docker service container.
    pub async fn stop_service(&self, state: &DockerState) -> Result<()> {
        container::stop_container(&self.docker, &state.container_id, 10).await?;
        tracing::info!(container = %state.container_name, "container stopped");
        Ok(())
    }

    /// Stop and remove a single docker service container.
    pub async fn delete_service(&self, state: &DockerState) -> Result<()> {
        container::stop_container(&self.docker, &state.container_id, 10).await?;
        container::remove_container(&self.docker, &state.container_id, true).await?;
        tracing::info!(container = %state.container_name, "container removed");
        Ok(())
    }

    /// Remove all Docker resources (containers, volumes, networks) for this project.
    pub async fn cleanup_all(&self) -> Result<()> {
        // Remove containers by label
        let containers = container::list_project_containers(&self.docker, &self.slug).await?;
        for c in &containers {
            if let Some(id) = &c.id {
                container::stop_container(&self.docker, id, 5).await?;
                container::remove_container(&self.docker, id, true).await?;
            }
        }

        // Remove volumes by label
        volume::remove_project_volumes(&self.docker, &self.slug).await?;

        // Remove network
        let network_name = self.network_name();
        network::remove_network(&self.docker, &network_name).await?;

        Ok(())
    }

    /// Check if Docker daemon is available and reachable.
    pub async fn ensure_docker_available(&self) -> Result<()> {
        self.docker
            .ping()
            .await
            .context("Cannot connect to Docker daemon. Is Docker running?")?;
        Ok(())
    }
}
