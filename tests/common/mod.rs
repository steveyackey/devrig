#![allow(dead_code)]
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct TestProject {
    pub dir: TempDir,
    pub config_path: PathBuf,
}

impl TestProject {
    pub fn new(config_toml: &str) -> Self {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("devrig.toml");
        std::fs::write(&config_path, config_toml).unwrap();
        Self { dir, config_path }
    }
}

pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

pub fn free_ports(count: usize) -> Vec<u16> {
    // Bind all at once to avoid reuse, then drop
    let listeners: Vec<_> = (0..count)
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let ports: Vec<_> = listeners
        .iter()
        .map(|l| l.local_addr().unwrap().port())
        .collect();
    drop(listeners);
    ports
}

pub async fn wait_for_port(port: u16, timeout: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    false
}

pub async fn wait_for_port_release(port: u16, timeout: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    false
}

/// Read the project slug from .devrig/state.json.
pub fn read_slug(project: &TestProject) -> Option<String> {
    let state_path = project.dir.path().join(".devrig/state.json");
    let content = std::fs::read_to_string(state_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v["slug"].as_str().map(|s| s.to_string())
}

/// Clean up Docker resources by label as a fallback after devrig delete.
/// Uses Docker CLI directly so it works even if devrig state is corrupted.
pub fn docker_cleanup(slug: &str) {
    // Remove containers
    let output = std::process::Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("label=devrig.project={}", slug),
            "--format",
            "{{.ID}}",
        ])
        .output();
    if let Ok(output) = output {
        let ids = String::from_utf8_lossy(&output.stdout);
        for id in ids.lines().filter(|l| !l.is_empty()) {
            let _ = std::process::Command::new("docker")
                .args(["rm", "-f", id])
                .output();
        }
    }

    // Remove volumes
    let output = std::process::Command::new("docker")
        .args([
            "volume",
            "ls",
            "--filter",
            &format!("label=devrig.project={}", slug),
            "--format",
            "{{.Name}}",
        ])
        .output();
    if let Ok(output) = output {
        let names = String::from_utf8_lossy(&output.stdout);
        for name in names.lines().filter(|l| !l.is_empty()) {
            let _ = std::process::Command::new("docker")
                .args(["volume", "rm", "-f", name])
                .output();
        }
    }

    // Remove networks
    let output = std::process::Command::new("docker")
        .args([
            "network",
            "ls",
            "--filter",
            &format!("label=devrig.project={}", slug),
            "--format",
            "{{.Name}}",
        ])
        .output();
    if let Ok(output) = output {
        let names = String::from_utf8_lossy(&output.stdout);
        for name in names.lines().filter(|l| !l.is_empty()) {
            let _ = std::process::Command::new("docker")
                .args(["network", "rm", name])
                .output();
        }
    }
}

/// Check if k3d is available on the system.
pub fn k3d_available() -> bool {
    std::process::Command::new("k3d")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Clean up a k3d cluster by name. Ignores errors (best-effort cleanup).
pub async fn k3d_cleanup(cluster_name: &str) {
    let _ = tokio::process::Command::new("k3d")
        .args(["cluster", "delete", cluster_name])
        .output()
        .await;
}

/// Synchronous version of k3d_cleanup, safe to call from scopeguard Drop.
/// Does NOT create a tokio runtime, so it works inside async contexts.
pub fn k3d_cleanup_sync(cluster_name: &str) {
    let _ = std::process::Command::new("k3d")
        .args(["cluster", "delete", cluster_name])
        .output();
}

/// Wait for a pod matching a label selector to be in Running state.
pub async fn wait_for_pod_running(
    kubeconfig: &Path,
    label: &str,
    timeout: std::time::Duration,
) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        let output = tokio::process::Command::new("kubectl")
            .args([
                "get",
                "pods",
                "-l",
                label,
                "--kubeconfig",
                &kubeconfig.to_string_lossy(),
                "-o",
                "jsonpath={.items[0].status.phase}",
            ])
            .output()
            .await;

        if let Ok(output) = output {
            let phase = String::from_utf8_lossy(&output.stdout);
            if phase.trim() == "Running" {
                return true;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    }
    false
}

/// Wait for a Job to complete (Succeeded status).
pub async fn wait_for_job_complete(
    kubeconfig: &Path,
    job_name: &str,
    timeout: std::time::Duration,
) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        let output = tokio::process::Command::new("kubectl")
            .args([
                "get",
                "job",
                job_name,
                "--kubeconfig",
                &kubeconfig.to_string_lossy(),
                "-o",
                "jsonpath={.status.succeeded}",
            ])
            .output()
            .await;

        if let Ok(output) = output {
            let succeeded = String::from_utf8_lossy(&output.stdout);
            if succeeded.trim() == "1" {
                return true;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    }
    false
}

/// Compute SHA-256 checksum of a file. Returns None if file doesn't exist.
pub fn file_checksum(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let content = std::fs::read(path).ok()?;
    let hash = Sha256::digest(&content);
    Some(hex::encode(hash))
}
