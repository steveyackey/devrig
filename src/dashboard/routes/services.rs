use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::config::model::DevrigConfig;
use crate::orchestrator::state::ProjectState;

use super::DashboardState;

#[derive(Debug, Serialize)]
pub struct ServiceInfo {
    pub name: String,
    pub port: Option<u16>,
    pub kind: String,
    pub port_auto: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

pub async fn get_services(
    State(state): State<DashboardState>,
) -> Json<Vec<ServiceInfo>> {
    let mut services = Vec::new();

    // Load runtime state (services, docker, compose)
    if let Some(project) = state
        .state_dir
        .as_ref()
        .and_then(|dir| ProjectState::load(dir))
    {
        for (name, svc) in &project.services {
            services.push(ServiceInfo {
                name: name.clone(),
                port: svc.port,
                kind: "service".to_string(),
                port_auto: svc.port_auto,
                protocol: svc.protocol.clone(),
                phase: svc.phase.clone(),
                exit_code: svc.exit_code,
                addon_type: None,
                url: None,
            });
        }

        for (name, docker_svc) in &project.docker {
            services.push(ServiceInfo {
                name: name.clone(),
                port: docker_svc.port,
                kind: "docker".to_string(),
                port_auto: docker_svc.port_auto,
                protocol: docker_svc.protocol.clone(),
                phase: Some("running".to_string()),
                exit_code: None,
                addon_type: None,
                url: None,
            });
        }

        for (name, compose) in &project.compose_services {
            services.push(ServiceInfo {
                name: name.clone(),
                port: compose.port,
                kind: "compose".to_string(),
                port_auto: false,
                protocol: None,
                phase: Some("running".to_string()),
                exit_code: None,
                addon_type: None,
                url: None,
            });
        }
    }

    // Load config for links, addons, and cluster ports
    if let Some(config_path) = &state.config_path {
        if let Ok(content) = std::fs::read_to_string(config_path) {
            if let Ok(config) = toml::from_str::<DevrigConfig>(&content) {
                // Links
                for (name, url) in &config.links {
                    let port = parse_port_from_url(url);
                    services.push(ServiceInfo {
                        name: name.clone(),
                        port,
                        kind: "link".to_string(),
                        port_auto: false,
                        protocol: None,
                        phase: None,
                        exit_code: None,
                        addon_type: None,
                        url: Some(url.clone()),
                    });
                }

                // Cluster addons and ports
                if let Some(cluster) = &config.cluster {
                    for (name, addon) in &cluster.addons {
                        let port_forwards = addon.parsed_port_forwards();
                        let port = port_forwards.first().map(|(p, _)| *p);
                        services.push(ServiceInfo {
                            name: name.clone(),
                            port,
                            kind: "addon".to_string(),
                            port_auto: false,
                            protocol: None,
                            phase: None,
                            exit_code: None,
                            addon_type: Some(addon.addon_type().to_string()),
                            url: None,
                        });
                    }

                    // Cluster port mappings (e.g. "8080:30080")
                    for mapping in &cluster.ports {
                        let parts: Vec<&str> = mapping.split(':').collect();
                        if let Some(host_port) = parts.first().and_then(|p| p.parse::<u16>().ok())
                        {
                            services.push(ServiceInfo {
                                name: format!("cluster:{}", mapping),
                                port: Some(host_port),
                                kind: "cluster-port".to_string(),
                                port_auto: false,
                                protocol: None,
                                phase: None,
                                exit_code: None,
                                addon_type: None,
                                url: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // Sort by kind then name
    services.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));

    Json(services)
}

fn parse_port_from_url(url: &str) -> Option<u16> {
    // Extract port from URLs like "http://localhost:8080" or "http://localhost:8080/path"
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
    let port_str = host_port.rsplit(':').next()?;
    port_str.parse::<u16>().ok()
}
