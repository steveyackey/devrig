use anyhow::Result;
use std::path::Path;

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let config_path = cwd.join("devrig.toml");

    if config_path.exists() {
        anyhow::bail!("devrig.toml already exists in {}", cwd.display());
    }

    // Detect project type
    let project_name = cwd
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "my-project".to_string());

    let (service_name, service_command) = detect_project_type(&cwd);

    let config = format!(
        r#"[project]
name = "{project_name}"

# Global environment variables shared by all services
# [env]
# DATABASE_URL = "postgres://localhost/mydb"

[services.{service_name}]
command = "{service_command}"
# port = 3000
# path = "./"

# Add more services:
# [services.worker]
# command = "cargo run --bin worker"
# depends_on = ["{service_name}"]
"#
    );

    std::fs::write(&config_path, &config)?;
    println!("Created devrig.toml in {}", cwd.display());
    println!();
    println!("  Project: {}", project_name);
    println!("  Service: {} -> {}", service_name, service_command);
    println!();
    println!("Edit the file, then run `devrig start` to begin.");
    Ok(())
}

fn detect_project_type(dir: &Path) -> (&'static str, &'static str) {
    if dir.join("Cargo.toml").exists() {
        ("app", "cargo watch -x run")
    } else if dir.join("package.json").exists() {
        ("app", "npm run dev")
    } else if dir.join("go.mod").exists() {
        ("app", "go run .")
    } else if dir.join("requirements.txt").exists() || dir.join("pyproject.toml").exists() {
        ("app", "python main.py")
    } else {
        ("app", "echo 'Replace this with your command'")
    }
}
