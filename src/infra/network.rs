use anyhow::{Context, Result};
use bollard::models::{EndpointSettings, NetworkConnectRequest, NetworkCreateRequest};
use bollard::Docker;
use std::collections::HashMap;

/// Create a project-scoped bridge network if it doesn't already exist.
pub async fn ensure_network(
    docker: &Docker,
    network_name: &str,
    labels: HashMap<String, String>,
) -> Result<()> {
    match docker.inspect_network(network_name, None).await {
        Ok(_) => return Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {}
        Err(e) => return Err(e).context("inspecting network"),
    }

    let config = NetworkCreateRequest {
        name: network_name.to_string(),
        driver: Some("bridge".to_string()),
        labels: Some(labels),
        ..Default::default()
    };
    docker
        .create_network(config)
        .await
        .context("creating Docker network")?;
    Ok(())
}

/// Remove a Docker network, ignoring 404 (already removed).
pub async fn remove_network(docker: &Docker, network_name: &str) -> Result<()> {
    match docker.remove_network(network_name).await {
        Ok(()) => Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(()),
        Err(e) => Err(e).context("removing Docker network"),
    }
}

/// Connect a container to a network with optional aliases.
pub async fn connect_container(
    docker: &Docker,
    network_name: &str,
    container_id: &str,
    aliases: Vec<String>,
) -> Result<()> {
    let config = NetworkConnectRequest {
        container: container_id.to_string(),
        endpoint_config: Some(EndpointSettings {
            aliases: Some(aliases),
            ..Default::default()
        }),
    };
    docker
        .connect_network(network_name, config)
        .await
        .context("connecting container to network")?;
    Ok(())
}

/// Build the standard set of devrig labels for a Docker resource.
pub fn resource_labels(slug: &str, service: &str) -> HashMap<String, String> {
    HashMap::from([
        ("devrig.project".to_string(), slug.to_string()),
        ("devrig.service".to_string(), service.to_string()),
        ("devrig.managed-by".to_string(), "devrig".to_string()),
    ])
}
