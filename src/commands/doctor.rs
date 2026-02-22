use anyhow::Result;
use std::process::Command;

pub fn run() -> Result<()> {
    println!("devrig doctor");
    println!("=============");
    println!();

    let checks = [
        ("docker", &["--version"] as &[&str]),
        ("docker-compose", &["compose", "version"]),
        ("k3d", &["--version"]),
        ("kubectl", &["version", "--client", "--short"]),
        ("cargo-watch", &["watch", "--version"]),
    ];

    let mut all_ok = true;

    for (name, args) in &checks {
        // Special cases: cargo-watch uses 'cargo' binary, docker-compose uses 'docker' binary
        let (bin, cmd_args, display_name) = if *name == "cargo-watch" {
            ("cargo", *args, *name)
        } else if *name == "docker-compose" {
            ("docker", *args, "docker compose")
        } else {
            (*name, *args, *name)
        };

        match Command::new(bin).args(cmd_args).output() {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout);
                let version = version.trim();
                // Some tools output to stderr
                let version = if version.is_empty() {
                    String::from_utf8_lossy(&output.stderr).trim().to_string()
                } else {
                    version.to_string()
                };
                println!("  [ok] {:<20} {}", display_name, version);
            }
            _ => {
                println!("  [!!] {:<20} not found", display_name);
                all_ok = false;
            }
        }
    }

    println!();
    if all_ok {
        println!("All dependencies found.");
    } else {
        println!("Some dependencies are missing. Install them for full functionality.");
        println!("Note: docker, docker compose, and k3d are only needed for infrastructure services (v0.2+).");
    }

    Ok(())
}
