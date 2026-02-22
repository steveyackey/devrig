pub mod env;
pub mod logs;
pub mod metrics;
pub mod services;
pub mod status;
pub mod traces;

use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use tokio::sync::{broadcast, RwLock};

use crate::otel::storage::TelemetryStore;
use crate::otel::types::TelemetryEvent;

#[derive(Clone)]
pub struct DashboardState {
    pub store: Arc<RwLock<TelemetryStore>>,
    pub events_tx: broadcast::Sender<TelemetryEvent>,
}

pub fn api_router(state: DashboardState) -> Router {
    Router::new()
        .route("/api/traces", get(traces::list_traces))
        .route("/api/traces/{trace_id}", get(traces::get_trace))
        .route("/api/traces/{trace_id}/related", get(traces::get_related))
        .route("/api/logs", get(logs::list_logs))
        .route("/api/metrics", get(metrics::list_metrics))
        .route("/api/status", get(status::get_status))
        .with_state(state)
}
