use anyhow::{Context, Result};
use bollard::models::VolumeCreateRequest;
use bollard::query_parameters::{ListVolumesOptions, RemoveVolumeOptions};
use bollard::Docker;
use std::collections::HashMap;

/// Parse a volume spec like "pgdata:/var/lib/postgresql/data" into
/// (project-scoped volume name, container path).
pub fn parse_volume_spec(spec: &str, slug: &str) -> Option<(String, String)> {
    let (name, path) = spec.split_once(':')?;
    if name.is_empty() || path.is_empty() {
        return None;
    }
    let scoped_name = format!("devrig-{}-{}", slug, name);
    Some((scoped_name, path.to_string()))
}

/// Create a Docker volume if it doesn't already exist.
pub async fn ensure_volume(
    docker: &Docker,
    volume_name: &str,
    labels: HashMap<String, String>,
) -> Result<()> {
    match docker.inspect_volume(volume_name).await {
        Ok(_) => return Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {}
        Err(e) => return Err(e).context("inspecting volume"),
    }

    let config = VolumeCreateRequest {
        name: Some(volume_name.to_string()),
        labels: Some(labels),
        ..Default::default()
    };
    docker
        .create_volume(config)
        .await
        .context("creating Docker volume")?;
    Ok(())
}

/// Remove a Docker volume, ignoring 404 (already removed).
pub async fn remove_volume(docker: &Docker, volume_name: &str) -> Result<()> {
    let options = RemoveVolumeOptions { force: false };
    match docker.remove_volume(volume_name, Some(options)).await {
        Ok(()) => Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(()),
        Err(e) => Err(e).context("removing Docker volume"),
    }
}

/// List all volumes with the devrig.project label matching the given slug.
pub async fn list_project_volumes(
    docker: &Docker,
    slug: &str,
) -> Result<Vec<bollard::models::Volume>> {
    let filters = HashMap::from([(
        "label".to_string(),
        vec![format!("devrig.project={}", slug)],
    )]);
    let options = ListVolumesOptions {
        filters: Some(filters),
    };
    let response = docker
        .list_volumes(Some(options))
        .await
        .context("listing project volumes")?;
    Ok(response.volumes.unwrap_or_default())
}

/// Remove all volumes with the devrig.project label matching the given slug.
pub async fn remove_project_volumes(docker: &Docker, slug: &str) -> Result<()> {
    let volumes = list_project_volumes(docker, slug).await?;
    for vol in volumes {
        tracing::debug!(volume = %vol.name, "removing volume");
        remove_volume(docker, &vol.name).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_volume_spec_basic() {
        let (name, path) =
            parse_volume_spec("pgdata:/var/lib/postgresql/data", "myapp-abc123").unwrap();
        assert_eq!(name, "devrig-myapp-abc123-pgdata");
        assert_eq!(path, "/var/lib/postgresql/data");
    }

    #[test]
    fn parse_volume_spec_empty_name() {
        assert!(parse_volume_spec(":/var/lib", "slug").is_none());
    }

    #[test]
    fn parse_volume_spec_empty_path() {
        assert!(parse_volume_spec("name:", "slug").is_none());
    }

    #[test]
    fn parse_volume_spec_no_colon() {
        assert!(parse_volume_spec("justname", "slug").is_none());
    }
}
