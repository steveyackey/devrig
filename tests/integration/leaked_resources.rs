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

fn count_docker_resources(slug: &str) -> (usize, usize, usize) {
    // Count containers
    let containers = std::process::Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("label=devrig.project={slug}"),
            "--format",
            "{{.ID}}",
        ])
        .output()
        .unwrap();
    let container_count = String::from_utf8_lossy(&containers.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .count();

    // Count volumes
    let volumes = std::process::Command::new("docker")
        .args([
            "volume",
            "ls",
            "--filter",
            &format!("label=devrig.project={slug}"),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .unwrap();
    let volume_count = String::from_utf8_lossy(&volumes.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .count();

    // Count networks
    let networks = std::process::Command::new("docker")
        .args([
            "network",
            "ls",
            "--filter",
            &format!("label=devrig.project={slug}"),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .unwrap();
    let network_count = String::from_utf8_lossy(&networks.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .count();

    (container_count, volume_count, network_count)
}

#[tokio::test]
async fn no_leaked_resources_after_delete() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-leak-check"

[infra.redis]
image = "redis:7-alpine"
port = {port}
volumes = ["redisdata:/data"]
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
        "Redis should be reachable on port {port}"
    );

    // Get slug from state
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

    // Resources should exist
    let (containers, volumes, networks) = count_docker_resources(&slug);
    assert!(containers > 0, "Should have running containers");
    assert!(volumes > 0, "Should have volumes");
    assert!(networks > 0, "Should have network");

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

    // Wait briefly for Docker to clean up
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Zero leaked resources
    let (containers, volumes, networks) = count_docker_resources(&slug);
    assert_eq!(
        containers, 0,
        "No containers should remain after delete for slug {slug}"
    );
    assert_eq!(
        volumes, 0,
        "No volumes should remain after delete for slug {slug}"
    );
    assert_eq!(
        networks, 0,
        "No networks should remain after delete for slug {slug}"
    );

    // Fallback cleanup via Docker CLI
    docker_cleanup(&slug);
}
