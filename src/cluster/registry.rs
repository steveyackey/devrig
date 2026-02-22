use anyhow::{bail, Context, Result};
use backon::{ExponentialBuilder, Retryable};
use std::time::Duration;
use tokio::process::Command;
use tracing::info;

/// Look up the host port for the k3d-managed registry container via `docker inspect`.
///
/// The registry container is named `k3d-devrig-{slug}-reg` (k3d prepends "k3d-" to the
/// name given in `--registry-create`).
pub async fn get_registry_port(slug: &str) -> Result<u16> {
    let container = format!("k3d-devrig-{}-reg", slug);
    let output = Command::new("docker")
        .args([
            "inspect",
            &container,
            "--format",
            "{{(index .NetworkSettings.Ports \"5000/tcp\" 0).HostPort}}",
        ])
        .output()
        .await
        .context("running docker inspect for registry port")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "docker inspect for registry '{}' failed: {}",
            container,
            stderr.trim()
        );
    }

    let port_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    port_str.parse::<u16>().with_context(|| {
        format!(
            "parsing registry port from docker inspect output: '{}'",
            port_str
        )
    })
}

/// Wait for the local registry to become ready by polling its `/v2/` endpoint
/// with exponential backoff. Gives up after 15 seconds.
pub async fn wait_for_registry(port: u16) -> Result<()> {
    let url = format!("http://localhost:{}/v2/", port);

    let result = tokio::time::timeout(Duration::from_secs(15), async {
        let url = url.clone();
        (|| async {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .context("building HTTP client")?;
            let response = client.get(&url).send().await.context("GET /v2/")?;
            if !response.status().is_success() {
                bail!("registry returned status {}", response.status());
            }
            Ok(())
        })
        .retry(
            ExponentialBuilder::default()
                .with_min_delay(Duration::from_millis(250))
                .with_max_delay(Duration::from_secs(3))
                .without_max_times(),
        )
        .await
    })
    .await;

    match result {
        Ok(Ok(())) => {
            info!(port, "registry is ready");
            Ok(())
        }
        Ok(Err(e)) => Err(e),
        Err(_) => bail!("registry on port {} did not become ready within 15s", port),
    }
}
