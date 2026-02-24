use anyhow::{Context, Result};
use bollard::models::VolumeCreateRequest;
use bollard::query_parameters::{ListVolumesOptions, RemoveVolumeOptions};
use bollard::Docker;
use std::collections::HashMap;

/// Result of parsing a volume spec — either a named Docker volume or a host
/// bind mount.
#[derive(Debug, Clone, PartialEq)]
pub enum VolumeSpec {
    /// Named volume: `"pgdata:/var/lib/postgresql/data"` — devrig creates and
    /// manages a Docker volume scoped to the project.
    Named {
        volume_name: String,
        container_path: String,
    },
    /// Bind mount: `"/host/path:/container/path"` — mounts a host directory
    /// directly into the container. No Docker volume is created.
    Bind {
        host_path: String,
        container_path: String,
    },
}

/// Parse a volume spec into a [`VolumeSpec`].
///
/// Bind mounts are detected when the source starts with `/`, `./`, or `../`.
/// Everything else is treated as a named volume and scoped to the project.
pub fn parse_volume_spec(spec: &str, slug: &str) -> Option<VolumeSpec> {
    let (source, path) = spec.split_once(':')?;
    if source.is_empty() || path.is_empty() {
        return None;
    }

    if is_bind_mount(source) {
        Some(VolumeSpec::Bind {
            host_path: source.to_string(),
            container_path: path.to_string(),
        })
    } else {
        let scoped_name = format!("devrig-{}-{}", slug, source);
        Some(VolumeSpec::Named {
            volume_name: scoped_name,
            container_path: path.to_string(),
        })
    }
}

/// Returns true if the source portion of a volume spec looks like a host path
/// (absolute or relative) rather than a named volume.
fn is_bind_mount(source: &str) -> bool {
    source.starts_with('/')
        || source.starts_with("./")
        || source.starts_with("../")
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
    fn parse_named_volume() {
        let spec =
            parse_volume_spec("pgdata:/var/lib/postgresql/data", "myapp-abc123").unwrap();
        assert_eq!(
            spec,
            VolumeSpec::Named {
                volume_name: "devrig-myapp-abc123-pgdata".to_string(),
                container_path: "/var/lib/postgresql/data".to_string(),
            }
        );
    }

    #[test]
    fn parse_bind_mount_absolute() {
        let spec =
            parse_volume_spec("/home/user/data:/var/lib/postgresql/data", "slug").unwrap();
        assert_eq!(
            spec,
            VolumeSpec::Bind {
                host_path: "/home/user/data".to_string(),
                container_path: "/var/lib/postgresql/data".to_string(),
            }
        );
    }

    #[test]
    fn parse_bind_mount_relative_dot() {
        let spec = parse_volume_spec("./data:/app/data", "slug").unwrap();
        assert_eq!(
            spec,
            VolumeSpec::Bind {
                host_path: "./data".to_string(),
                container_path: "/app/data".to_string(),
            }
        );
    }

    #[test]
    fn parse_bind_mount_relative_dotdot() {
        let spec = parse_volume_spec("../shared:/app/shared", "slug").unwrap();
        assert_eq!(
            spec,
            VolumeSpec::Bind {
                host_path: "../shared".to_string(),
                container_path: "/app/shared".to_string(),
            }
        );
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
