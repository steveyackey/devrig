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
async fn delete_removes_volumes() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-vol-cleanup"

[infra.postgres]
image = "postgres:16-alpine"
port = {port}
volumes = ["pgdata:/var/lib/postgresql/data"]
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
        "Postgres should be reachable on port {port}"
    );

    // Wait for state to get slug
    let state_file = project.dir.path().join(".devrig").join("state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let state_json = std::fs::read_to_string(&state_file).expect("state.json should exist");
    let state: serde_json::Value = serde_json::from_str(&state_json).unwrap();
    let slug = state["slug"].as_str().unwrap().to_string();

    // Verify volume exists
    let vol_check = std::process::Command::new("docker")
        .args(["volume", "inspect", &format!("devrig-{}-pgdata", slug)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(vol_check.success(), "Volume should exist while running");

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

    // Delete
    let delete = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config_path_str])
        .output()
        .expect("failed to run delete");
    assert!(
        delete.status.success(),
        "delete should succeed: {}",
        String::from_utf8_lossy(&delete.stderr)
    );

    // Verify volume is removed
    let vol_check = std::process::Command::new("docker")
        .args(["volume", "inspect", &format!("devrig-{}-pgdata", slug)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(
        !vol_check.success(),
        "Volume should be removed after delete"
    );

    // Fallback cleanup via Docker CLI
    docker_cleanup(&slug);
}
