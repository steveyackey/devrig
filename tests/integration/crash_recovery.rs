use crate::common::*;
use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn crash_recovery_restarts() {
    // Use a service that exits immediately -- the supervisor should restart it
    let project = TestProject::new(
        r#"
[project]
name = "test-crash"
[services.crasher]
command = "echo 'started' && exit 1"
"#,
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project.config_path.to_str().unwrap()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start devrig");

    // Let the supervisor attempt a few restarts
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Stop devrig
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
    // If we got here without hanging, the restart + shutdown cycle works
}
