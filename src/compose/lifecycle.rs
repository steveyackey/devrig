use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;

/// Represents a service reported by `docker compose ps --format json`.
#[derive(Debug, Deserialize)]
pub struct ComposeService {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Service")]
    pub service: String,
    #[serde(rename = "State")]
    pub state: String,
    #[serde(default, rename = "Health")]
    pub health: String,
    #[serde(default, rename = "Publishers")]
    pub publishers: Vec<ComposePublisher>,
}

#[derive(Debug, Deserialize)]
pub struct ComposePublisher {
    #[serde(rename = "TargetPort")]
    pub target_port: u16,
    #[serde(rename = "PublishedPort")]
    pub published_port: u16,
}

/// Run `docker compose up -d` for the specified services.
pub async fn compose_up(
    compose_file: &Path,
    project_name: &str,
    services: &[String],
    env_file: Option<&str>,
) -> Result<()> {
    let mut cmd = tokio::process::Command::new("docker");
    cmd.args([
        "compose",
        "-f",
        &compose_file.to_string_lossy(),
        "-p",
        project_name,
        "up",
        "-d",
    ]);
    if let Some(ef) = env_file {
        cmd.args(["--env-file", ef]);
    }
    for svc in services {
        cmd.arg(svc);
    }

    let output = cmd.output().await.context("running docker compose up")?;
    if !output.status.success() {
        bail!(
            "docker compose up failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Run `docker compose down --remove-orphans`.
pub async fn compose_down(compose_file: &Path, project_name: &str) -> Result<()> {
    let output = tokio::process::Command::new("docker")
        .args([
            "compose",
            "-f",
            &compose_file.to_string_lossy(),
            "-p",
            project_name,
            "down",
            "--remove-orphans",
        ])
        .output()
        .await
        .context("running docker compose down")?;

    if !output.status.success() {
        bail!(
            "docker compose down failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Run `docker compose ps --format json` and parse the output.
pub async fn compose_ps(compose_file: &Path, project_name: &str) -> Result<Vec<ComposeService>> {
    let output = tokio::process::Command::new("docker")
        .args([
            "compose",
            "-f",
            &compose_file.to_string_lossy(),
            "-p",
            project_name,
            "ps",
            "--format",
            "json",
        ])
        .output()
        .await
        .context("running docker compose ps")?;

    if !output.status.success() {
        bail!(
            "docker compose ps failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // docker compose ps --format json may output one JSON object per line
    // or a JSON array depending on the version
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    // Try parsing as a JSON array first
    if let Ok(services) = serde_json::from_str::<Vec<ComposeService>>(trimmed) {
        return Ok(services);
    }

    // Fall back to newline-delimited JSON objects
    let mut services = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let svc: ComposeService =
            serde_json::from_str(line).context("parsing docker compose ps output")?;
        services.push(svc);
    }

    Ok(services)
}
