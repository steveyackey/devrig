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
async fn network_isolation() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port1 = free_port();
    let port2 = free_port();

    let project1 = TestProject::new(&format!(
        r#"
[project]
name = "test-net-iso-a"

[infra.redis]
image = "redis:7-alpine"
port = {port1}
ready_check = {{ type = "tcp" }}
"#
    ));

    let project2 = TestProject::new(&format!(
        r#"
[project]
name = "test-net-iso-b"

[infra.redis]
image = "redis:7-alpine"
port = {port2}
ready_check = {{ type = "tcp" }}
"#
    ));

    let config1 = project1.config_path.to_str().unwrap().to_string();
    let config2 = project2.config_path.to_str().unwrap().to_string();

    // Start both projects
    let mut child1 = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config1])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start project 1");

    let mut child2 = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config2])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start project 2");

    assert!(
        wait_for_port(port1, Duration::from_secs(60)).await,
        "Project 1 redis should be reachable"
    );
    assert!(
        wait_for_port(port2, Duration::from_secs(60)).await,
        "Project 2 redis should be reachable"
    );

    // Wait for state files before stopping
    let state_file_1 = project1.dir.path().join(".devrig/state.json");
    let state_file_2 = project2.dir.path().join(".devrig/state.json");
    let start_time = std::time::Instant::now();
    while start_time.elapsed() < Duration::from_secs(10) {
        if state_file_1.exists() && state_file_2.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Both should have different networks
    let state1 = std::fs::read_to_string(project1.dir.path().join(".devrig/state.json"))
        .expect("state1 should exist");
    let state2 = std::fs::read_to_string(project2.dir.path().join(".devrig/state.json"))
        .expect("state2 should exist");

    let s1: serde_json::Value = serde_json::from_str(&state1).unwrap();
    let s2: serde_json::Value = serde_json::from_str(&state2).unwrap();

    let slug1 = s1["slug"].as_str().unwrap();
    let slug2 = s2["slug"].as_str().unwrap();

    assert_ne!(slug1, slug2, "Projects should have different slugs");

    // Verify networks exist and are different
    let net1 = format!("devrig-{}-net", slug1);
    let net2 = format!("devrig-{}-net", slug2);
    assert_ne!(net1, net2, "Network names should be different");

    let net1_check = std::process::Command::new("docker")
        .args(["network", "inspect", &net1])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(net1_check.success(), "Network 1 should exist");

    let net2_check = std::process::Command::new("docker")
        .args(["network", "inspect", &net2])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(net2_check.success(), "Network 2 should exist");

    // Stop both
    #[cfg(unix)]
    {
        for child in [&child1, &child2] {
            let pid = child.id().unwrap();
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGINT,
            )
            .ok();
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(20), child1.wait()).await;
    let _ = tokio::time::timeout(Duration::from_secs(20), child2.wait()).await;

    // Delete both
    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config1])
        .output();
    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config2])
        .output();

    // Verify networks are removed
    let net1_check = std::process::Command::new("docker")
        .args(["network", "inspect", &net1])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!net1_check.success(), "Network 1 should be removed");

    let net2_check = std::process::Command::new("docker")
        .args(["network", "inspect", &net2])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!net2_check.success(), "Network 2 should be removed");

    // Fallback cleanup via Docker CLI
    docker_cleanup(slug1);
    docker_cleanup(slug2);
}
