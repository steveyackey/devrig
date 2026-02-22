use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::orchestrator::state::ProjectState;

use super::DashboardState;

#[derive(Debug, Serialize)]
pub struct ServiceInfo {
    pub name: String,
    pub port: Option<u16>,
    pub kind: String,
    pub port_auto: bool,
}

pub async fn get_services(
    State(state): State<DashboardState>,
) -> Json<Vec<ServiceInfo>> {
    let Some(state_dir) = &state.state_dir else {
        return Json(vec![]);
    };

    let Some(project) = ProjectState::load(state_dir) else {
        return Json(vec![]);
    };

    let mut services = Vec::new();

    for (name, svc) in &project.services {
        services.push(ServiceInfo {
            name: name.clone(),
            port: svc.port,
            kind: "service".to_string(),
            port_auto: svc.port_auto,
        });
    }

    for (name, infra) in &project.infra {
        services.push(ServiceInfo {
            name: name.clone(),
            port: infra.port,
            kind: "infra".to_string(),
            port_auto: infra.port_auto,
        });
    }

    for (name, compose) in &project.compose_services {
        services.push(ServiceInfo {
            name: name.clone(),
            port: compose.port,
            kind: "compose".to_string(),
            port_auto: false,
        });
    }

    // Sort by kind then name
    services.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));

    Json(services)
}
