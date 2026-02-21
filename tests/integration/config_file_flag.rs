use crate::common::*;
use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn custom_config_file() {
    let port = free_port();
    let dir = tempfile::TempDir::new().unwrap();
    let custom_path = dir.path().join("custom.toml");
    std::fs::write(
        &custom_path,
        format!(
            r#"
[project]
name = "test-custom"
[services.web]
command = "python3 -m http.server {port}"
port = {port}
"#
        ),
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["-f", custom_path.to_str().unwrap(), "start"])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(
        wait_for_port(port, Duration::from_secs(10)).await,
        "Service did not start with custom config"
    );

    #[cfg(unix)]
    {
        let pid = child.id().unwrap();
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .ok();
    }
    let _ = tokio::time::timeout(Duration::from_secs(15), child.wait()).await;
}
