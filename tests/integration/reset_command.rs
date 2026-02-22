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
async fn reset_clears_init_flag() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-reset"

[infra.redis]
image = "redis:7-alpine"
port = {port}
ready_check = {{ type = "tcp" }}
"#
    ));

    let config_path_str = project.config_path.to_str().unwrap().to_string();
    let state_dir = project.dir.path().join(".devrig");

    // Start to create state
    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    // Wait for the port to be ready (container is running and healthy)
    assert!(
        wait_for_port(port, Duration::from_secs(60)).await,
        "Redis container should be reachable on port {}",
        port
    );

    // Also wait for state file to be written
    let state_file = state_dir.join("state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(state_file.exists(), "state.json should exist after start");

    // Stop devrig (SIGINT)
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

    // Run reset
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["reset", "-f", &config_path_str, "redis"])
        .output()
        .expect("failed to run reset");

    assert!(
        output.status.success(),
        "reset should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Reset init flag"),
        "Should confirm reset: {}",
        stdout
    );

    // Verify state file was updated
    let state_json =
        std::fs::read_to_string(state_dir.join("state.json")).expect("state.json should exist");
    assert!(
        state_json.contains("\"init_completed\": false")
            || state_json.contains("\"init_completed\":false"),
        "init_completed should be false after reset: {}",
        state_json
    );

    // Read slug before delete removes state
    let slug = read_slug(&project);

    // Cleanup: always run delete to remove Docker resources
    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config_path_str])
        .output();

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}
