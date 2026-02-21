use crate::common::*;
use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn start_stop_lifecycle() {
    let port = free_port();
    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-lifecycle"
[services.web]
command = "python3 -m http.server {port}"
port = {port}
"#
    ));

    // Start devrig as a child process
    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project.config_path.to_str().unwrap()])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    // Wait for port to become reachable
    assert!(
        wait_for_port(port, Duration::from_secs(10)).await,
        "Service did not become reachable on port {port}"
    );

    // Send SIGINT to trigger the ctrl_c handler in devrig
    #[cfg(unix)]
    {
        let pid = child.id().unwrap();
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .ok();
    }

    // Wait for process to exit
    let _status = tokio::time::timeout(Duration::from_secs(15), child.wait())
        .await
        .expect("devrig did not exit in time")
        .expect("failed to wait on devrig");

    // Verify port is released
    assert!(
        wait_for_port_release(port, Duration::from_secs(5)).await,
        "Port {port} was not released after stop"
    );
}
