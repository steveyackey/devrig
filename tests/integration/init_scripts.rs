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
async fn init_scripts_run_once() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-init-scripts"

[infra.postgres]
image = "postgres:16-alpine"
port = {port}
ready_check = {{ type = "pg_isready" }}
init = ["CREATE TABLE IF NOT EXISTS test_init(id int);"]

[infra.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"
"#
    ));

    let config_path_str = project.config_path.to_str().unwrap().to_string();

    // First start -- init scripts should run
    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(
        wait_for_port(port, Duration::from_secs(60)).await,
        "Postgres should be reachable on port {port}"
    );

    // Wait for state file
    let state_dir = project.dir.path().join(".devrig");
    let state_file = state_dir.join("state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Verify init_completed is true
    let state_json = std::fs::read_to_string(&state_file).expect("state.json should exist");
    assert!(
        state_json.contains("\"init_completed\": true")
            || state_json.contains("\"init_completed\":true"),
        "init_completed should be true after first start: {}",
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

    // Reset init flag
    let reset_output = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["reset", "-f", &config_path_str, "postgres"])
        .output()
        .expect("failed to run reset");
    assert!(
        reset_output.status.success(),
        "reset should succeed: {}",
        String::from_utf8_lossy(&reset_output.stderr)
    );

    // Verify init_completed is now false
    let state_json = std::fs::read_to_string(&state_file).expect("state.json should exist");
    assert!(
        state_json.contains("\"init_completed\": false")
            || state_json.contains("\"init_completed\":false"),
        "init_completed should be false after reset: {}",
        state_json
    );

    // Read slug before delete removes state
    let slug = read_slug(&project);

    // Cleanup
    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config_path_str])
        .output();

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}
