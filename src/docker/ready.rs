use anyhow::{bail, Context, Result};
use backon::{ExponentialBuilder, Retryable};
use bollard::query_parameters::LogsOptions;
use bollard::Docker;
use futures_util::StreamExt;
use std::time::Duration;

use crate::config::model::ReadyCheck;
use crate::docker::exec::exec_in_container;

/// Run a ready check for a container, retrying with exponential backoff.
///
/// Dispatches to the appropriate strategy based on the ReadyCheck variant.
pub async fn run_ready_check(
    docker: &Docker,
    container_id: &str,
    check: &ReadyCheck,
    host_port: Option<u16>,
    docker_name: &str,
) -> Result<()> {
    let total_timeout = Duration::from_secs(check.timeout_secs());

    match check {
        ReadyCheck::Log { pattern, .. } => {
            run_log_check(docker, container_id, pattern, total_timeout, docker_name).await
        }
        _ => {
            let docker = docker.clone();
            let container_id = container_id.to_string();
            let check = check.clone();
            let docker_name = docker_name.to_string();

            let result = tokio::time::timeout(total_timeout, async {
                (|| async { run_single_check(&docker, &container_id, &check, host_port).await })
                    .retry(
                        ExponentialBuilder::default()
                            .with_min_delay(Duration::from_millis(250))
                            .with_max_delay(Duration::from_secs(3))
                            .with_max_times(200)
                            .with_jitter(),
                    )
                    .notify(|err: &anyhow::Error, dur: Duration| {
                        tracing::debug!(
                            docker = %docker_name,
                            "ready check failed: {}, retrying in {:?}",
                            err,
                            dur
                        );
                    })
                    .await
            })
            .await;

            match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(e),
                Err(_) => bail!(
                    "ready check for '{}' timed out after {:?}",
                    docker_name,
                    total_timeout
                ),
            }
        }
    }
}

/// Run a single (non-retrying) check based on the strategy.
async fn run_single_check(
    docker: &Docker,
    container_id: &str,
    check: &ReadyCheck,
    host_port: Option<u16>,
) -> Result<()> {
    match check {
        ReadyCheck::PgIsReady { .. } => {
            let cmd = vec![
                "pg_isready".to_string(),
                "-h".to_string(),
                "localhost".to_string(),
                "-q".to_string(),
                "-t".to_string(),
                "2".to_string(),
            ];
            let (exit_code, _) = exec_in_container(docker, container_id, cmd).await?;
            if exit_code != 0 {
                bail!("pg_isready returned exit code {}", exit_code);
            }
            Ok(())
        }
        ReadyCheck::Cmd { command, expect, .. } => {
            crate::docker::exec::exec_ready_check(docker, container_id, command, expect.as_deref())
                .await
        }
        ReadyCheck::Http { url, .. } => {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .context("building HTTP client")?;
            let response = client.get(url).send().await.context("HTTP ready check")?;
            if !response.status().is_success() {
                bail!("HTTP ready check returned status {}", response.status());
            }
            Ok(())
        }
        ReadyCheck::Tcp { .. } => {
            let port = host_port.context("TCP ready check requires a port")?;
            tokio::time::timeout(
                Duration::from_secs(2),
                tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)),
            )
            .await
            .context("TCP connect timed out")?
            .context("TCP connect failed")?;
            Ok(())
        }
        ReadyCheck::Log { .. } => {
            unreachable!("log check handled separately")
        }
    }
}

/// Run a log-based ready check by streaming container logs and scanning for
/// a pattern match.
async fn run_log_check(
    docker: &Docker,
    container_id: &str,
    pattern: &str,
    timeout: Duration,
    docker_name: &str,
) -> Result<()> {
    let options = LogsOptions {
        follow: true,
        stdout: true,
        stderr: true,
        tail: "all".to_string(),
        ..Default::default()
    };

    let mut stream = docker.logs(container_id, Some(options));

    let result = tokio::time::timeout(timeout, async {
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(log) => {
                    let text = log.to_string();
                    if text.contains(pattern) {
                        return Ok(());
                    }
                }
                Err(e) => {
                    tracing::warn!(docker = %docker_name, "log stream error: {}", e);
                }
            }
        }
        bail!("log stream ended without finding pattern '{}'", pattern)
    })
    .await;

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => bail!(
            "log ready check for '{}' timed out after {:?} (pattern: '{}')",
            docker_name,
            timeout,
            pattern
        ),
    }
}
