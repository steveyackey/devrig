use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::orchestrator::state::ProjectState;

use super::DashboardState;

#[derive(Debug, Serialize)]
pub struct ClusterResponse {
    pub cluster_name: String,
    pub kubeconfig_path: String,
    pub registry: Option<RegistryInfo>,
    pub deployed_services: Vec<DeployedServiceInfo>,
    pub addons: Vec<AddonInfo>,
}

#[derive(Debug, Serialize)]
pub struct RegistryInfo {
    pub name: String,
    pub port: u16,
}

#[derive(Debug, Serialize)]
pub struct DeployedServiceInfo {
    pub name: String,
    pub image_tag: String,
    pub last_deployed: String,
}

#[derive(Debug, Serialize)]
pub struct AddonInfo {
    pub name: String,
    pub addon_type: String,
    pub namespace: String,
    pub installed_at: String,
}

pub async fn get_cluster(
    State(state): State<DashboardState>,
) -> Json<Option<ClusterResponse>> {
    let Some(state_dir) = &state.state_dir else {
        return Json(None);
    };

    let Some(project) = ProjectState::load(state_dir) else {
        return Json(None);
    };

    let Some(cluster) = &project.cluster else {
        return Json(None);
    };

    let registry = match (&cluster.registry_name, cluster.registry_port) {
        (Some(name), Some(port)) => Some(RegistryInfo {
            name: name.clone(),
            port,
        }),
        _ => None,
    };

    let deployed_services = cluster
        .deployed_services
        .iter()
        .map(|(name, deploy)| DeployedServiceInfo {
            name: name.clone(),
            image_tag: deploy.image_tag.clone(),
            last_deployed: deploy.last_deployed.to_rfc3339(),
        })
        .collect();

    let addons = cluster
        .installed_addons
        .iter()
        .map(|(name, addon)| AddonInfo {
            name: name.clone(),
            addon_type: addon.addon_type.clone(),
            namespace: addon.namespace.clone(),
            installed_at: addon.installed_at.to_rfc3339(),
        })
        .collect();

    Json(Some(ClusterResponse {
        cluster_name: cluster.cluster_name.clone(),
        kubeconfig_path: cluster.kubeconfig_path.clone(),
        registry,
        deployed_services,
        addons,
    }))
}
