use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::config;
use crate::config::interpolate::{build_template_vars, resolve_config_templates};
use crate::discovery::env::build_service_env;
use crate::orchestrator::state::ProjectState;

pub fn run(config_path: Option<&Path>, service_name: &str) -> Result<()> {
    let config_path = match config_path {
        Some(p) => p.to_path_buf(),
        None => crate::config::resolve::resolve_config(None)?,
    };

    let mut config = config::load_config(&config_path)?;

    if !config.services.contains_key(service_name) {
        bail!(
            "unknown service '{}' (available: {:?})",
            service_name,
            config.services.keys().collect::<Vec<_>>()
        );
    }

    let project_dir = config_path.parent().unwrap_or(Path::new("."));
    let state_dir = ProjectState::state_dir_for(project_dir);
    let state = ProjectState::load(&state_dir);

    let mut resolved_ports: HashMap<String, u16> = HashMap::new();
    if let Some(ref s) = state {
        for (name, svc_state) in &s.services {
            if let Some(port) = svc_state.port {
                resolved_ports.insert(format!("service:{}", name), port);
            }
        }
        for (name, infra_state) in &s.infra {
            if let Some(port) = infra_state.port {
                resolved_ports.insert(format!("infra:{}", name), port);
            }
            for (pname, &port) in &infra_state.named_ports {
                resolved_ports.insert(format!("infra:{}:{}", name, pname), port);
            }
        }
        for (name, cs_state) in &s.compose_services {
            if let Some(port) = cs_state.port {
                resolved_ports.insert(format!("compose:{}", name), port);
            }
        }
    }

    let template_vars = build_template_vars(&config, &resolved_ports);
    let _ = resolve_config_templates(&mut config, &template_vars);

    let mut env = build_service_env(service_name, &config, &resolved_ports);

    // Add compose service discovery vars (mirrors orchestrator behavior)
    if let Some(ref s) = state {
        for (cs_name, cs_state) in &s.compose_services {
            let upper = cs_name.to_uppercase();
            env.insert(format!("DEVRIG_{}_HOST", upper), "localhost".to_string());
            if let Some(port) = cs_state.port {
                env.insert(format!("DEVRIG_{}_PORT", upper), port.to_string());
                env.insert(
                    format!("DEVRIG_{}_URL", upper),
                    format!("http://localhost:{}", port),
                );
            }
        }
    }

    for (key, value) in &env {
        println!("{}={}", key, value);
    }

    Ok(())
}
