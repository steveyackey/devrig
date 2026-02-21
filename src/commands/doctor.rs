use anyhow::Result;
use std::process::Command;

pub fn run() -> Result<()> {
    println!("devrig doctor");
    println!("=============");
    println!();

    let checks = [
        ("docker", &["--version"] as &[&str]),
        ("k3d", &["--version"]),
        ("kubectl", &["version", "--client", "--short"]),
        ("cargo-watch", &["watch", "--version"]),
    ];

    let mut all_ok = true;

    for (name, args) in &checks {
        // For cargo-watch, the binary is 'cargo' with subcommand 'watch'
        let (bin, cmd_args) = if *name == "cargo-watch" {
            ("cargo", *args)
        } else {
            (*name, *args)
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
                println!("  [ok] {:<16} {}", name, version);
            }
            _ => {
                println!("  [!!] {:<16} not found", name);
                all_ok = false;
            }
        }
    }

    println!();
    if all_ok {
        println!("All dependencies found.");
    } else {
        println!("Some dependencies are missing. Install them for full functionality.");
        println!("Note: docker and k3d are only needed for infrastructure services (v0.2+).");
    }

    Ok(())
}
