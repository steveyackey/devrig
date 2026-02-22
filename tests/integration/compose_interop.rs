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

fn compose_available() -> bool {
    std::process::Command::new("docker")
        .args(["compose", "version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn compose_basic() {
    if !docker_available() || !compose_available() {
        eprintln!("Skipping: Docker or Docker Compose not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(
        r#"
[project]
name = "test-compose"

[compose]
file = "docker-compose.yml"
services = ["redis"]
"#,
    );

    // Create a minimal docker-compose.yml in the project dir
    let compose_content = format!(
        r#"services:
  redis:
    image: redis:7-alpine
    ports:
      - "{port}:6379"
"#
    );
    std::fs::write(
        project.dir.path().join("docker-compose.yml"),
        compose_content,
    )
    .expect("failed to write compose file");

    let config_path_str = project.config_path.to_str().unwrap().to_string();

    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(
        wait_for_port(port, Duration::from_secs(60)).await,
        "Compose redis should be reachable on port {port}"
    );

    // Verify state has compose_services
    let state_file = project.dir.path().join(".devrig").join("state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let state_json = std::fs::read_to_string(&state_file).expect("state.json should exist");
    assert!(
        state_json.contains("compose_services"),
        "State should have compose_services section: {}",
        state_json
    );

    // Stop
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

    // Delete
    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config_path_str])
        .output();

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}
