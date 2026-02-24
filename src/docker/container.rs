use anyhow::{Context, Result};
use bollard::models::{ContainerCreateBody, HostConfig, PortBinding};
use bollard::query_parameters::{
    CreateContainerOptions, ListContainersOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::Docker;
use std::collections::HashMap;

use crate::docker::network::resource_labels;

/// Port mapping: (container_port, host_port).
pub struct PortMap {
    pub container_port: u16,
    pub host_port: u16,
}

/// Options for overriding a container's command and entrypoint.
#[derive(Default)]
pub struct ContainerCmdOptions {
    /// Override the container's CMD (command to run).
    pub cmd: Option<Vec<String>>,
    /// Override the container's ENTRYPOINT.
    pub entrypoint: Option<Vec<String>>,
}

/// Create a Docker container with the specified configuration.
#[allow(clippy::too_many_arguments)]
pub async fn create_container(
    docker: &Docker,
    slug: &str,
    service_name: &str,
    image: &str,
    env_vars: &[(String, String)],
    port_maps: &[PortMap],
    volumes: &[(String, String)],
    network_name: &str,
    cmd_options: &ContainerCmdOptions,
) -> Result<String> {
    let container_name = format!("devrig-{}-{}", slug, service_name);
    let labels = resource_labels(slug, service_name);

    let env: Vec<String> = env_vars
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();

    let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    let mut exposed_ports: Vec<String> = Vec::new();
    for pm in port_maps {
        let container_port_key = format!("{}/tcp", pm.container_port);
        port_bindings.insert(
            container_port_key.clone(),
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(pm.host_port.to_string()),
            }]),
        );
        exposed_ports.push(container_port_key);
    }

    let binds: Vec<String> = volumes
        .iter()
        .map(|(vol, path)| format!("{}:{}", vol, path))
        .collect();

    let host_config = HostConfig {
        port_bindings: Some(port_bindings),
        binds: Some(binds),
        network_mode: Some(network_name.to_string()),
        ..Default::default()
    };

    let config = ContainerCreateBody {
        image: Some(image.to_string()),
        env: Some(env),
        exposed_ports: Some(exposed_ports),
        host_config: Some(host_config),
        labels: Some(labels),
        cmd: cmd_options.cmd.clone(),
        entrypoint: cmd_options.entrypoint.clone(),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: Some(container_name.clone()),
        ..Default::default()
    };

    // Remove existing container with same name (idempotent restart)
    let _ = remove_container(docker, &container_name, true).await;

    let response = docker
        .create_container(Some(options), config)
        .await
        .with_context(|| format!("creating container {}", container_name))?;

    tracing::debug!(
        container = %container_name,
        id = %response.id,
        "container created"
    );

    Ok(response.id)
}

/// Start a container by ID.
pub async fn start_container(docker: &Docker, container_id: &str) -> Result<()> {
    docker
        .start_container(container_id, None::<StartContainerOptions>)
        .await
        .with_context(|| format!("starting container {}", container_id))?;
    Ok(())
}

/// Stop a container by name or ID with a timeout.
pub async fn stop_container(docker: &Docker, container_id: &str, timeout_secs: i32) -> Result<()> {
    let options = StopContainerOptions {
        t: Some(timeout_secs),
        signal: None,
    };
    match docker.stop_container(container_id, Some(options)).await {
        Ok(()) => Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 304, ..
        }) => Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(()),
        Err(e) => Err(e).context("stopping container"),
    }
}

/// Remove a container by name or ID.
pub async fn remove_container(docker: &Docker, container_id: &str, force: bool) -> Result<()> {
    let options = RemoveContainerOptions {
        force,
        ..Default::default()
    };
    match docker.remove_container(container_id, Some(options)).await {
        Ok(()) => Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(()),
        Err(e) => Err(e).context("removing container"),
    }
}

/// List all containers with the devrig.project label matching the given slug.
pub async fn list_project_containers(
    docker: &Docker,
    slug: &str,
) -> Result<Vec<bollard::models::ContainerSummary>> {
    let filters = HashMap::from([(
        "label".to_string(),
        vec![format!("devrig.project={}", slug)],
    )]);
    let options = ListContainersOptions {
        all: true,
        filters: Some(filters),
        ..Default::default()
    };
    docker
        .list_containers(Some(options))
        .await
        .context("listing project containers")
}
