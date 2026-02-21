use crate::common::*;
use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn two_projects_no_crosstalk() {
    let ports = free_ports(2);

    let project_a = TestProject::new(&format!(
        r#"
[project]
name = "project-a"
[services.web]
command = "python3 -m http.server {}"
port = {}
"#,
        ports[0], ports[0]
    ));

    let project_b = TestProject::new(&format!(
        r#"
[project]
name = "project-b"
[services.web]
command = "python3 -m http.server {}"
port = {}
"#,
        ports[1], ports[1]
    ));

    // Start both
    let mut child_a = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project_a.config_path.to_str().unwrap()])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start project a");

    let mut child_b = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project_b.config_path.to_str().unwrap()])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start project b");

    // Both should be reachable
    assert!(
        wait_for_port(ports[0], Duration::from_secs(10)).await,
        "Project A not reachable"
    );
    assert!(
        wait_for_port(ports[1], Duration::from_secs(10)).await,
        "Project B not reachable"
    );

    // Stop A, B should still be running
    #[cfg(unix)]
    {
        let pid = child_a.id().unwrap();
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .ok();
    }
    let _ = tokio::time::timeout(Duration::from_secs(15), child_a.wait()).await;

    // B should still be running
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        wait_for_port(ports[1], Duration::from_secs(2)).await,
        "Project B should still be running"
    );

    // Stop B
    #[cfg(unix)]
    {
        let pid = child_b.id().unwrap();
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .ok();
    }
    let _ = tokio::time::timeout(Duration::from_secs(15), child_b.wait()).await;
}
