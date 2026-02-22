#![allow(dead_code)]
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
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
