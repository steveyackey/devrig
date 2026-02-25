pub mod cluster;
pub mod config;
pub mod env;
pub mod logs;
pub mod metrics;
pub mod services;
pub mod status;
pub mod traces;

use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::{broadcast, RwLock};

use crate::otel::storage::TelemetryStore;
use crate::otel::types::TelemetryEvent;

#[derive(Clone)]
pub struct DashboardState {
    pub store: Arc<RwLock<TelemetryStore>>,
    pub events_tx: broadcast::Sender<TelemetryEvent>,
    pub config_path: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
}

pub fn api_router(state: DashboardState) -> Router {
    Router::new()
        .route("/api/traces", get(traces::list_traces))
        .route("/api/traces/{trace_id}", get(traces::get_trace))
        .route("/api/traces/{trace_id}/related", get(traces::get_related))
        .route("/api/logs", get(logs::list_logs))
        .route("/api/metrics", get(metrics::list_metrics))
        .route("/api/metrics/series", get(metrics::get_metric_series))
        .route("/api/status", get(status::get_status))
        .route(
            "/api/config",
            get(config::get_config).put(config::update_config),
        )
        .route("/api/services", get(services::get_services))
        .route("/api/cluster", get(cluster::get_cluster))
        .route("/api/config/validate", post(config::validate_config))
        .with_state(state)
}
