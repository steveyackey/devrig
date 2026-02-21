use crate::orchestrator::registry::InstanceRegistry;
use crate::orchestrator::state::ProjectState;
use anyhow::Result;
use std::path::Path;

pub fn run(config_path: Option<&Path>, all: bool) -> Result<()> {
    if all {
        run_all()
    } else {
        run_local(config_path)
    }
}

fn run_local(config_path: Option<&Path>) -> Result<()> {
    // Resolve config path to find state dir
    let config_path = match config_path {
        Some(p) => p.to_path_buf(),
        None => crate::config::resolve::resolve_config(None)?,
    };
    let project_dir = config_path.parent().unwrap_or(Path::new("."));
    let state_dir = ProjectState::state_dir_for(project_dir);

    let state = match ProjectState::load(&state_dir) {
        Some(s) => s,
        None => {
            println!("No running services found.");
            println!("Run `devrig start` to start services.");
            return Ok(());
        }
    };

    println!(
        "  Project: {} (started {})",
        state.slug,
        state.started_at.format("%Y-%m-%d %H:%M:%S")
    );
    println!();
    println!("  {:<16} {:<8} {:<24} STATUS", "SERVICE", "PID", "URL");
    println!("  {}", "-".repeat(60));

    for (name, svc) in &state.services {
        let url = svc
            .port
            .map(|p| format!("http://localhost:{}", p))
            .unwrap_or_else(|| "-".to_string());
        let auto_tag = if svc.port_auto { " (auto)" } else { "" };
        let alive = is_process_alive(svc.pid);
        let status = if alive { "running" } else { "stopped" };
        println!(
            "  {:<16} {:<8} {:<24} {}",
            name,
            svc.pid,
            format!("{}{}", url, auto_tag),
            status
        );
    }
    println!();
    Ok(())
}

fn run_all() -> Result<()> {
    let mut registry = InstanceRegistry::load();
    registry.cleanup();
    let _ = registry.save();

    let instances = registry.list();
    if instances.is_empty() {
        println!("No running devrig instances found.");
        return Ok(());
    }

    println!("  {:<24} {:<40} STATUS", "PROJECT", "CONFIG");
    println!("  {}", "-".repeat(70));

    for entry in instances {
        let state = ProjectState::load(&std::path::PathBuf::from(&entry.state_dir));
        let svc_count = state.as_ref().map(|s| s.services.len()).unwrap_or(0);
        let status = if svc_count > 0 {
            format!("{} services", svc_count)
        } else {
            "unknown".to_string()
        };
        println!("  {:<24} {:<40} {}", entry.slug, entry.config_path, status);
    }
    println!();
    Ok(())
}

fn is_process_alive(pid: u32) -> bool {
    // Check if process is alive via kill(0) on Unix
    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        kill(Pid::from_raw(pid as i32), None).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
