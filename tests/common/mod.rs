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
