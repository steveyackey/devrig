use crate::common::*;
use std::time::Duration;
use tokio::process::Command;

fn docker_available() -> bool {
    std::process::Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn ready_check_tcp_with_redis() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-ready-tcp"

[infra.redis]
image = "redis:7-alpine"
port = {port}
ready_check = {{ type = "tcp" }}
"#
    ));

    let config_path_str = project.config_path.to_str().unwrap().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(
        wait_for_port(port, Duration::from_secs(60)).await,
        "Redis should be reachable via TCP check on port {port}"
    );

    // Wait for state file before stopping
    let state_file = project.dir.path().join(".devrig/state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[cfg(unix)]
    {
        let pid = child.id().unwrap();
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .ok();
    }
    let _ = tokio::time::timeout(Duration::from_secs(20), child.wait()).await;

    // Read slug before delete removes state
    let slug = read_slug(&project);

    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config_path_str])
        .output();

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}

#[tokio::test]
async fn ready_check_cmd_with_redis() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-ready-cmd"

[infra.redis]
image = "redis:7-alpine"
port = {port}

[infra.redis.ready_check]
type = "cmd"
command = "redis-cli ping"
expect = "PONG"
"#
    ));

    let config_path_str = project.config_path.to_str().unwrap().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(
        wait_for_port(port, Duration::from_secs(60)).await,
        "Redis should be reachable after cmd ready check on port {port}"
    );

    // Wait for state file before stopping
    let state_file = project.dir.path().join(".devrig/state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[cfg(unix)]
    {
        let pid = child.id().unwrap();
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .ok();
    }
    let _ = tokio::time::timeout(Duration::from_secs(20), child.wait()).await;

    // Read slug before delete removes state
    let slug = read_slug(&project);

    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config_path_str])
        .output();

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}

#[tokio::test]
async fn ready_check_pg_isready() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-ready-pg"

[infra.postgres]
image = "postgres:16-alpine"
port = {port}
ready_check = {{ type = "pg_isready" }}

[infra.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"
"#
    ));

    let config_path_str = project.config_path.to_str().unwrap().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(
        wait_for_port(port, Duration::from_secs(60)).await,
        "Postgres should be reachable after pg_isready check on port {port}"
    );

    // Wait for state file before stopping
    let state_file = project.dir.path().join(".devrig/state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[cfg(unix)]
    {
        let pid = child.id().unwrap();
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .ok();
    }
    let _ = tokio::time::timeout(Duration::from_secs(20), child.wait()).await;

    // Read slug before delete removes state
    let slug = read_slug(&project);

    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config_path_str])
        .output();

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}

#[tokio::test]
async fn ready_check_log_with_postgres() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-ready-log"

[infra.postgres]
image = "postgres:16-alpine"
port = {port}

[infra.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"

[infra.postgres.ready_check]
type = "log"
match = "ready to accept connections"
"#
    ));

    let config_path_str = project.config_path.to_str().unwrap().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(
        wait_for_port(port, Duration::from_secs(60)).await,
        "Postgres should be reachable after log ready check on port {port}"
    );

    // Wait for state file before stopping
    let state_file = project.dir.path().join(".devrig/state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[cfg(unix)]
    {
        let pid = child.id().unwrap();
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .ok();
    }
    let _ = tokio::time::timeout(Duration::from_secs(20), child.wait()).await;

    // Read slug before delete removes state
    let slug = read_slug(&project);

    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config_path_str])
        .output();

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}
