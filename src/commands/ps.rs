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

    // Docker containers
    if !state.docker.is_empty() {
        println!("  {:<20} {:<14} {:<24} STATUS", "INFRA", "CONTAINER", "URL");
        println!("  {}", "-".repeat(68));
        for (name, docker_svc) in &state.docker {
            let url = docker_svc
                .port
                .map(|p| format!("localhost:{}", p))
                .unwrap_or_else(|| "-".to_string());
            let auto_tag = if docker_svc.port_auto { " (auto)" } else { "" };
            let short_id = if docker_svc.container_id.len() > 12 {
                &docker_svc.container_id[..12]
            } else {
                &docker_svc.container_id
            };
            let init_tag = if docker_svc.init_completed { " [init]" } else { "" };
            println!(
                "  {:<20} {:<14} {:<24} running{}",
                name,
                short_id,
                format!("{}{}", url, auto_tag),
                init_tag,
            );
        }
        println!();
    }

    // Compose services
    if !state.compose_services.is_empty() {
        println!(
            "  {:<20} {:<14} {:<24} STATUS",
            "COMPOSE", "CONTAINER", "URL"
        );
        println!("  {}", "-".repeat(68));
        for (name, cs) in &state.compose_services {
            let url = cs
                .port
                .map(|p| format!("localhost:{}", p))
                .unwrap_or_else(|| "-".to_string());
            let short_id = if cs.container_id.len() > 12 {
                &cs.container_id[..12]
            } else {
                &cs.container_id
            };
            println!("  {:<20} {:<14} {:<24} running", name, short_id, url,);
        }
        println!();
    }

    // Dashboard
    if let Some(ref dash) = state.dashboard {
        println!("  {:<20} {:<24}", "DASHBOARD", "URL");
        println!("  {}", "-".repeat(48));
        println!(
            "  {:<20} http://localhost:{}",
            "dashboard", dash.dashboard_port
        );
        println!(
            "  {:<20} localhost:{}",
            "otel-grpc", dash.grpc_port
        );
        println!(
            "  {:<20} localhost:{}",
            "otel-http", dash.http_port
        );
        println!();
    }

    // Services
    if !state.services.is_empty() {
        println!("  {:<20} {:<8} {:<24} STATUS", "SERVICE", "PID", "URL");
        println!("  {}", "-".repeat(62));
        for (name, svc) in &state.services {
            let url = svc
                .port
                .map(|p| format!("http://localhost:{}", p))
                .unwrap_or_else(|| "-".to_string());
            let auto_tag = if svc.port_auto { " (auto)" } else { "" };
            let alive = is_process_alive(svc.pid);
            let status = if alive { "running" } else { "stopped" };
            println!(
                "  {:<20} {:<8} {:<24} {}",
                name,
                svc.pid,
                format!("{}{}", url, auto_tag),
                status
            );
        }
        println!();
    }

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
        let parts: Vec<String> = if let Some(ref s) = state {
            let mut p = Vec::new();
            if !s.services.is_empty() {
                p.push(format!("{} svc", s.services.len()));
            }
            if !s.docker.is_empty() {
                p.push(format!("{} docker", s.docker.len()));
            }
            if !s.compose_services.is_empty() {
                p.push(format!("{} compose", s.compose_services.len()));
            }
            p
        } else {
            vec![]
        };
        let status = if parts.is_empty() {
            "unknown".to_string()
        } else {
            parts.join(", ")
        };
        println!("  {:<24} {:<40} {}", entry.slug, entry.config_path, status);
    }
    println!();
    Ok(())
}

fn is_process_alive(pid: u32) -> bool {
    crate::platform::is_process_alive(pid)
}
