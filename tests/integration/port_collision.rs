use crate::common::*;
use std::net::TcpListener;

#[tokio::test]
async fn port_collision_detected() {
    let port = free_port();

    // Bind the port ourselves so it is occupied
    let _listener = TcpListener::bind(("127.0.0.1", port)).unwrap();

    let project = TestProject::new(&format!(
        r#"
[project]
name = "test-collision"
[services.web]
command = "echo hi"
port = {port}
"#
    ));

    // Try to start -- should fail with port conflict
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_devrig"))
        .args(["start", "-f", project.config_path.to_str().unwrap()])
        .output()
        .expect("failed to run devrig");

    assert!(
        !output.status.success(),
        "devrig should have failed due to port collision"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Port")
            || stderr.contains("port")
            || stderr.contains("conflict")
            || stderr.contains("already in use"),
        "Expected port conflict message, got: {}",
        stderr
    );
}
