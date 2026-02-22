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
async fn infra_container_start_and_stop() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-infra"

[infra.redis]
image = "redis:7-alpine"
port = {port}
ready_check = {{ type = "tcp" }}

[services.worker]
command = "sleep 30"
depends_on = ["redis"]
"#
    ));

    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project.config_path.to_str().unwrap()])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    // Wait for Redis port to become reachable
    assert!(
        wait_for_port(port, Duration::from_secs(30)).await,
        "Redis did not become reachable on port {port}"
    );

    // Verify state file has infra section
    let state_dir = project.dir.path().join(".devrig");
    let state_json =
        std::fs::read_to_string(state_dir.join("state.json")).expect("state.json should exist");
    assert!(
        state_json.contains("\"redis\""),
        "State should contain redis infra entry"
    );
    assert!(
        state_json.contains("container_id"),
        "State should contain container_id"
    );

    // Wait for state file before stopping
    let state_file = project.dir.path().join(".devrig/state.json");
    let start_time = std::time::Instant::now();
    while start_time.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Send SIGINT to trigger shutdown
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

    // Cleanup: delete to remove containers
    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", project.config_path.to_str().unwrap()])
        .output();

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}

#[tokio::test]
async fn infra_env_vars_injected() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-infra-env"

[infra.redis]
image = "redis:7-alpine"
port = {port}
ready_check = {{ type = "tcp" }}

[services.checker]
command = "env"
depends_on = ["redis"]
"#
    ));

    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project.config_path.to_str().unwrap()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    // Wait for Redis port
    assert!(wait_for_port(port, Duration::from_secs(30)).await);

    // Give the env command time to run and output
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Wait for state file before stopping
    let state_file = project.dir.path().join(".devrig/state.json");
    let start_time = std::time::Instant::now();
    while start_time.elapsed() < Duration::from_secs(10) {
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

    // Cleanup
    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", project.config_path.to_str().unwrap()])
        .output();

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}

#[tokio::test]
async fn delete_removes_containers() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-delete-infra"

[infra.redis]
image = "redis:7-alpine"
port = {port}
ready_check = {{ type = "tcp" }}
"#
    ));

    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project.config_path.to_str().unwrap()])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(wait_for_port(port, Duration::from_secs(30)).await);

    // Wait for state file before stopping
    let state_file = project.dir.path().join(".devrig/state.json");
    let start_time = std::time::Instant::now();
    while start_time.elapsed() < Duration::from_secs(10) {
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

    // Run delete
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", project.config_path.to_str().unwrap()])
        .output()
        .expect("failed to run delete");

    assert!(
        output.status.success(),
        "delete should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Port should be released (container removed)
    assert!(
        wait_for_port_release(port, Duration::from_secs(10)).await,
        "Port {port} was not released after delete"
    );

    // State dir should be gone
    let state_dir = project.dir.path().join(".devrig");
    assert!(
        !state_dir.exists(),
        "State dir should be removed after delete"
    );

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}
