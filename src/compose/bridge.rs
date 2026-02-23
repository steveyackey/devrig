use anyhow::Result;
use bollard::Docker;

use crate::compose::lifecycle::ComposeService;
use crate::docker::network;

/// Connect compose containers to the devrig project network so they can
/// communicate with native docker containers and be reached by services.
pub async fn bridge_compose_containers(
    docker: &Docker,
    network_name: &str,
    compose_containers: &[ComposeService],
) -> Result<()> {
    for container in compose_containers {
        tracing::debug!(
            container = %container.name,
            service = %container.service,
            "connecting compose container to devrig network"
        );
        // Ignore errors from already-connected containers
        let _ = network::connect_container(
            docker,
            network_name,
            &container.id,
            vec![container.service.clone()],
        )
        .await;
    }
    Ok(())
}
