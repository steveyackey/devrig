use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;

use crate::otel::storage::TelemetryStore;
use crate::otel::types::TelemetryEvent;

use super::routes::{self, DashboardState};
use super::static_files;
use super::ws;

pub async fn start_dashboard_server(
    port: u16,
    store: Arc<RwLock<TelemetryStore>>,
    events_tx: broadcast::Sender<TelemetryEvent>,
    cancel: CancellationToken,
    config_path: Option<PathBuf>,
) -> anyhow::Result<()> {
    let state = DashboardState {
        store,
        events_tx,
        config_path,
    };

    let app = routes::api_router(state.clone())
        .merge(ws::ws_router(state))
        .merge(static_files::static_router())
        .layer(CorsLayer::permissive());

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await?;

    Ok(())
}
