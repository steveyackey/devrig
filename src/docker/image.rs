use anyhow::{bail, Context, Result};
use bollard::auth::DockerCredentials;
use bollard::models::CreateImageInfo;
use bollard::query_parameters::CreateImageOptions;
use bollard::Docker;
use futures_util::StreamExt;

use crate::config::model::RegistryAuth;

/// Parse an image reference into (name, tag).
/// "postgres:16" -> ("postgres", "16")
/// "redis" -> ("redis", "latest")
/// "axllent/mailpit:latest" -> ("axllent/mailpit", "latest")
pub fn parse_image_ref(image: &str) -> (&str, &str) {
    match image.rsplit_once(':') {
        Some((name, tag)) if !name.is_empty() && !tag.is_empty() => (name, tag),
        _ => (image, "latest"),
    }
}

/// Check if an image exists locally.
pub async fn check_image_exists(docker: &Docker, image: &str) -> bool {
    docker.inspect_image(image).await.is_ok()
}

/// Pull a single Docker image with progress logging.
pub async fn pull_image(docker: &Docker, image: &str) -> Result<()> {
    let (name, tag) = parse_image_ref(image);
    tracing::debug!(image = %image, "pulling image");

    let options = CreateImageOptions {
        from_image: Some(name.to_string()),
        tag: Some(tag.to_string()),
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(options), None, None);
    while let Some(result) = stream.next().await {
        let info: CreateImageInfo = result.context("pulling image")?;
        if let Some(err) = &info.error_detail {
            bail!("image pull failed for {}: {:?}", image, err);
        }
    }

    tracing::debug!(image = %image, "image pulled successfully");
    Ok(())
}

/// Pull a single Docker image with optional registry authentication.
pub async fn pull_image_with_auth(
    docker: &Docker,
    image: &str,
    auth: Option<&RegistryAuth>,
) -> Result<()> {
    let (name, tag) = parse_image_ref(image);
    tracing::debug!(image = %image, "pulling image");

    let options = CreateImageOptions {
        from_image: Some(name.to_string()),
        tag: Some(tag.to_string()),
        ..Default::default()
    };

    let credentials = auth.map(|a| DockerCredentials {
        username: Some(a.username.clone()),
        password: Some(a.password.clone()),
        ..Default::default()
    });

    let mut stream = docker.create_image(Some(options), None, credentials);
    while let Some(result) = stream.next().await {
        let info: CreateImageInfo = result.context("pulling image")?;
        if let Some(err) = &info.error_detail {
            bail!("image pull failed for {}: {:?}", image, err);
        }
    }

    tracing::debug!(image = %image, "image pulled successfully");
    Ok(())
}

/// Pull multiple images in parallel, skipping those already present locally.
pub async fn pull_images_if_needed(docker: &Docker, images: &[&str]) -> Result<()> {
    let mut set = tokio::task::JoinSet::new();

    for &image in images {
        let docker = docker.clone();
        let image = image.to_string();
        set.spawn(async move {
            if check_image_exists(&docker, &image).await {
                tracing::debug!(image = %image, "image already present locally");
                return Ok(());
            }
            pull_image(&docker, &image).await
        });
    }

    while let Some(result) = set.join_next().await {
        result.context("image pull task panicked")??;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_image_with_tag() {
        assert_eq!(parse_image_ref("postgres:16"), ("postgres", "16"));
    }

    #[test]
    fn parse_image_without_tag() {
        assert_eq!(parse_image_ref("redis"), ("redis", "latest"));
    }

    #[test]
    fn parse_image_with_org_and_tag() {
        assert_eq!(
            parse_image_ref("axllent/mailpit:latest"),
            ("axllent/mailpit", "latest")
        );
    }

    #[test]
    fn parse_image_alpine() {
        assert_eq!(
            parse_image_ref("postgres:16-alpine"),
            ("postgres", "16-alpine")
        );
    }
}
