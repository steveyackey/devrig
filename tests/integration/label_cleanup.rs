use crate::common::*;
use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn delete_removes_state() {
    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-cleanup"
[services.web]
command = "python3 -m http.server {port}"
port = {port}
"#
    ));

    let state_dir = project.dir.path().join(".devrig");

    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project.config_path.to_str().unwrap()])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    assert!(wait_for_port(port, Duration::from_secs(10)).await);

    // State directory should exist
    assert!(state_dir.exists(), "State dir should exist while running");

    // Stop with SIGTERM
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

    // After stop, state should be cleaned up
    tokio::time::sleep(Duration::from_millis(500)).await;
    let state_file = state_dir.join("state.json");
    assert!(
        !state_file.exists(),
        "State file should be removed after stop"
    );
}
