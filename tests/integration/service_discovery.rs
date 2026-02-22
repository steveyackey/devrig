use crate::common::*;
use std::time::Duration;

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
async fn devrig_vars_injected_via_env_command() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let redis_port = free_port();
    let api_port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-svc-disc"

[infra.redis]
image = "redis:7-alpine"
port = {redis_port}
ready_check = {{ type = "tcp" }}

[services.api]
command = "sleep 30"
port = {api_port}
depends_on = ["redis"]
"#
    ));

    let config_path_str = project.config_path.to_str().unwrap().to_string();

    // Start devrig
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(
        wait_for_port(redis_port, Duration::from_secs(60)).await,
        "Redis should be reachable on port {redis_port}"
    );

    // Wait for state file
    let state_file = project.dir.path().join(".devrig").join("state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Use `devrig env` to check what env vars the api service gets
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["env", "-f", &config_path_str, "api"])
        .output()
        .expect("failed to run env command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify DEVRIG_REDIS_* vars
    assert!(
        stdout.contains("DEVRIG_REDIS_HOST=localhost"),
        "Should have DEVRIG_REDIS_HOST: {}",
        stdout
    );
    assert!(
        stdout.contains(&format!("DEVRIG_REDIS_PORT={redis_port}")),
        "Should have DEVRIG_REDIS_PORT={redis_port}: {}",
        stdout
    );
    assert!(
        stdout.contains("DEVRIG_REDIS_URL=redis://localhost:"),
        "Should have DEVRIG_REDIS_URL with redis:// protocol: {}",
        stdout
    );

    // Verify service gets its own PORT and HOST
    assert!(
        stdout.contains(&format!("PORT={api_port}")),
        "Should have PORT={api_port}: {}",
        stdout
    );
    assert!(
        stdout.contains("HOST=localhost"),
        "Should have HOST=localhost: {}",
        stdout
    );

    // Wait for state file before stopping
    let state_file_check = project.dir.path().join(".devrig/state.json");
    let start_time = std::time::Instant::now();
    while start_time.elapsed() < Duration::from_secs(10) {
        if state_file_check.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

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

    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["delete", "-f", &config_path_str])
        .output();

    // Fallback cleanup via Docker CLI
    if let Some(slug) = slug {
        docker_cleanup(&slug);
    }
}

#[tokio::test]
async fn url_generation_correctness() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let pg_port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-url-gen"

[infra.postgres]
image = "postgres:16-alpine"
port = {pg_port}
ready_check = {{ type = "pg_isready" }}

[infra.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "secret"

[services.api]
command = "sleep 30"
depends_on = ["postgres"]
"#
    ));

    let config_path_str = project.config_path.to_str().unwrap().to_string();

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(
        wait_for_port(pg_port, Duration::from_secs(60)).await,
        "Postgres should be reachable on port {pg_port}"
    );

    // Wait for state file
    let state_file = project.dir.path().join(".devrig").join("state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["env", "-f", &config_path_str, "api"])
        .output()
        .expect("failed to run env");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_url = format!("DEVRIG_POSTGRES_URL=postgres://devrig:secret@localhost:{pg_port}");
    assert!(
        stdout.contains(&expected_url),
        "Should have postgres URL with credentials: {}\nGot: {}",
        expected_url,
        stdout
    );

    // Wait for state file before stopping
    let state_file_check = project.dir.path().join(".devrig/state.json");
    let start_time = std::time::Instant::now();
    while start_time.elapsed() < Duration::from_secs(10) {
        if state_file_check.exists() {
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
async fn template_resolution_in_env() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let pg_port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-template"

[infra.postgres]
image = "postgres:16-alpine"
port = {pg_port}
ready_check = {{ type = "pg_isready" }}

[infra.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"

[services.api]
command = "sleep 30"
depends_on = ["postgres"]

[services.api.env]
DATABASE_URL = "postgres://devrig:devrig@localhost:{{{{ infra.postgres.port }}}}/myapp"
"#
    ));

    let config_path_str = project.config_path.to_str().unwrap().to_string();

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(
        wait_for_port(pg_port, Duration::from_secs(60)).await,
        "Postgres should be reachable on port {pg_port}"
    );

    // Wait for state file
    let state_file = project.dir.path().join(".devrig").join("state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if state_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["env", "-f", &config_path_str, "api"])
        .output()
        .expect("failed to run env");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected = format!("DATABASE_URL=postgres://devrig:devrig@localhost:{pg_port}/myapp");
    assert!(
        stdout.contains(&expected),
        "Template should resolve to actual port: {}\nGot: {}",
        expected,
        stdout
    );

    // Wait for state file before stopping
    let state_file_check = project.dir.path().join(".devrig/state.json");
    let start_time = std::time::Instant::now();
    while start_time.elapsed() < Duration::from_secs(10) {
        if state_file_check.exists() {
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
async fn auto_port_persistence() {
    if !docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let project = TestProject::new(
        r#"
[project]
name = "test-auto-port"

[infra.redis]
image = "redis:7-alpine"
port = "auto"
ready_check = { type = "tcp" }
"#,
    );

    let config_path_str = project.config_path.to_str().unwrap().to_string();

    // First start
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", &config_path_str])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    // Wait for state file to get the auto-assigned port
    let state_file = project.dir.path().join(".devrig").join("state.json");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        if state_file.exists() {
            let content = std::fs::read_to_string(&state_file).unwrap_or_default();
            if content.contains("port_auto") {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let state_json =
        std::fs::read_to_string(&state_file).expect("state.json should exist after start");
    // Extract the port from state
    let state: serde_json::Value =
        serde_json::from_str(&state_json).expect("state.json should be valid JSON");
    let first_port = state["infra"]["redis"]["port"]
        .as_u64()
        .expect("should have a port");
    assert!(first_port > 0, "auto port should be assigned");

    // Verify the port is actually reachable
    assert!(
        wait_for_port(first_port as u16, Duration::from_secs(30)).await,
        "Auto-assigned port {first_port} should be reachable"
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

    // Verify port_auto is true in state
    let state_json = std::fs::read_to_string(&state_file).expect("state should still exist");
    assert!(
        state_json.contains("\"port_auto\": true") || state_json.contains("\"port_auto\":true"),
        "port_auto should be true: {}",
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
