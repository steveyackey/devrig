use crate::common::*;

#[tokio::test]
async fn env_command_shows_vars() {
    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-env-cmd"

[env]
RUST_LOG = "debug"

[services.api]
command = "echo hi"
port = {port}

[services.api.env]
API_KEY = "secret123"
"#
    ));

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["env", "-f", project.config_path.to_str().unwrap(), "api"])
        .output()
        .expect("failed to run env command");

    assert!(
        output.status.success(),
        "env command should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("RUST_LOG=debug"),
        "Should contain global env: {}",
        stdout
    );
    assert!(
        stdout.contains("API_KEY=secret123"),
        "Should contain service env: {}",
        stdout
    );
    assert!(
        stdout.contains("HOST=localhost"),
        "Should contain HOST: {}",
        stdout
    );
}

#[tokio::test]
async fn env_command_unknown_service() {
    let project = TestProject::new(
        r#"
[project]
name = "test-env-unknown"

[services.api]
command = "echo hi"
"#,
    );

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args([
            "env",
            "-f",
            project.config_path.to_str().unwrap(),
            "nonexistent",
        ])
        .output()
        .expect("failed to run env command");

    assert!(!output.status.success(), "Should fail for unknown service");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown service"),
        "Should mention unknown service: {}",
        stderr
    );
}
