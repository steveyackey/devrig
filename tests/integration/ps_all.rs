use crate::common::*;
use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn ps_all_shows_instances() {
    let ports = free_ports(2);

    let project_a = TestProject::new(&format!(
        r#"
[project]
name = "ps-test-a"
[services.web]
command = "python3 -m http.server {}"
port = {}
"#,
        ports[0], ports[0]
    ));

    let project_b = TestProject::new(&format!(
        r#"
[project]
name = "ps-test-b"
[services.web]
command = "python3 -m http.server {}"
port = {}
"#,
        ports[1], ports[1]
    ));

    // Start project A first and wait for it to be fully up
    let mut child_a = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project_a.config_path.to_str().unwrap()])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start a");

    assert!(wait_for_port(ports[0], Duration::from_secs(10)).await);

    // Start project B after A is fully registered
    let mut child_b = Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project_b.config_path.to_str().unwrap()])
        .kill_on_drop(true)
        .spawn()
        .expect("failed to start b");

    assert!(wait_for_port(ports[1], Duration::from_secs(10)).await);

    // Small delay to ensure registry writes are flushed
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Run ps --all
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["ps", "--all"])
        .output()
        .expect("failed to run ps --all");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ps-test-a"),
        "ps --all should show project a: {}",
        stdout
    );
    assert!(
        stdout.contains("ps-test-b"),
        "ps --all should show project b: {}",
        stdout
    );

    // Cleanup
    #[cfg(unix)]
    {
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(child_a.id().unwrap() as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .ok();
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(child_b.id().unwrap() as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .ok();
    }
    let _ = tokio::time::timeout(Duration::from_secs(15), child_a.wait()).await;
    let _ = tokio::time::timeout(Duration::from_secs(15), child_b.wait()).await;
}
