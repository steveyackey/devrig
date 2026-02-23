use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;
use tracing::debug;

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

/// Discover service names from a docker-compose.yml file by parsing the
/// top-level `services:` section. This avoids requiring Docker at config
/// load time while still enabling auto-discovery of compose services as
/// valid `depends_on` targets.
///
/// Returns an empty vec if the file cannot be read or has no services section.
pub fn discover_compose_services(compose_file: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(compose_file) {
        Ok(c) => c,
        Err(e) => {
            debug!(path = %compose_file.display(), error = %e, "could not read compose file for service discovery");
            return Vec::new();
        }
    };

    let mut services = Vec::new();
    let mut in_services = false;
    let mut service_indent: Option<usize> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        if !in_services {
            if indent == 0 && trimmed.starts_with("services:") {
                in_services = true;
            }
            continue;
        }

        // Inside the services block
        if indent == 0 {
            // Hit another top-level key, done with services
            break;
        }

        match service_indent {
            None => {
                // First indented line — establishes the service-name indent level
                service_indent = Some(indent);
                if let Some(colon_pos) = trimmed.find(':') {
                    let name = &trimmed[..colon_pos];
                    if !name.is_empty() {
                        services.push(name.to_string());
                    }
                }
            }
            Some(si) if indent == si => {
                // Same indent level — another service name
                if let Some(colon_pos) = trimmed.find(':') {
                    let name = &trimmed[..colon_pos];
                    if !name.is_empty() {
                        services.push(name.to_string());
                    }
                }
            }
            _ => {
                // Deeper indent — properties of a service, skip
            }
        }
    }

    debug!(services = ?services, "discovered compose services from {}", compose_file.display());
    services
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn discover_services_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docker-compose.yml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            "services:\n  postgres:\n    image: postgres:16\n  redis:\n    image: redis:7"
        )
        .unwrap();

        let services = discover_compose_services(&path);
        assert_eq!(services, vec!["postgres", "redis"]);
    }

    #[test]
    fn discover_services_with_comments_and_blanks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docker-compose.yml");
        let content = "\
version: '3.8'

services:
  # Database
  postgres:
    image: postgres:16
    ports:
      - '5432:5432'

  mailpit:
    image: axllent/mailpit
    ports:
      - '1025:1025'

volumes:
  pgdata:
";
        std::fs::write(&path, content).unwrap();

        let services = discover_compose_services(&path);
        assert_eq!(services, vec!["postgres", "mailpit"]);
    }

    #[test]
    fn discover_services_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docker-compose.yml");
        std::fs::write(&path, "").unwrap();

        let services = discover_compose_services(&path);
        assert!(services.is_empty());
    }

    #[test]
    fn discover_services_no_services_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docker-compose.yml");
        std::fs::write(&path, "version: '3.8'\nvolumes:\n  pgdata:\n").unwrap();

        let services = discover_compose_services(&path);
        assert!(services.is_empty());
    }

    #[test]
    fn discover_services_missing_file() {
        let path = Path::new("/nonexistent/docker-compose.yml");
        let services = discover_compose_services(path);
        assert!(services.is_empty());
    }

    #[test]
    fn discover_services_tabs_indent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docker-compose.yml");
        let content = "services:\n\tdb:\n\t\timage: postgres\n\tcache:\n\t\timage: redis\n";
        std::fs::write(&path, content).unwrap();

        let services = discover_compose_services(&path);
        assert_eq!(services, vec!["db", "cache"]);
    }
}
