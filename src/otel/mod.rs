pub mod query;
pub mod receiver_grpc;
pub mod receiver_http;
pub mod storage;
pub mod types;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::config::model::OtelConfig;
use types::TelemetryEvent;

use self::storage::TelemetryStore;

/// Coordinator for OTLP gRPC + HTTP receivers and telemetry storage.
pub struct OtelCollector {
    store: Arc<RwLock<TelemetryStore>>,
    events_tx: broadcast::Sender<TelemetryEvent>,
    grpc_port: u16,
    http_port: u16,
}

impl OtelCollector {
    pub fn new(otel_config: &OtelConfig) -> Self {
        let retention = humantime::parse_duration(&otel_config.retention)
            .unwrap_or_else(|_| Duration::from_secs(3600));

        let store = Arc::new(RwLock::new(TelemetryStore::new(
            otel_config.trace_buffer,
            otel_config.log_buffer,
            otel_config.metric_buffer,
            retention,
        )));

        let (events_tx, _) = broadcast::channel(1024);

        Self {
            store,
            events_tx,
            grpc_port: otel_config.grpc_port.as_fixed().expect("otel grpc_port must be resolved before creating collector"),
            http_port: otel_config.http_port.as_fixed().expect("otel http_port must be resolved before creating collector"),
        }
    }

    pub fn store(&self) -> Arc<RwLock<TelemetryStore>> {
        Arc::clone(&self.store)
    }

    pub fn events_tx(&self) -> broadcast::Sender<TelemetryEvent> {
        self.events_tx.clone()
    }

    /// Start the OTLP gRPC and HTTP receivers as background tasks.
    /// Also starts a background sweeper for expired telemetry.
    pub async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        // Start gRPC OTLP receiver
        let grpc_store = Arc::clone(&self.store);
        let grpc_tx = self.events_tx.clone();
        let grpc_port = self.grpc_port;
        let grpc_cancel = cancel.clone();
        tokio::spawn(async move {
            if let Err(e) =
                receiver_grpc::start_grpc_server(grpc_port, grpc_store, grpc_tx, grpc_cancel).await
            {
                warn!(error = %e, "OTLP gRPC server failed");
            }
        });
        debug!(port = self.grpc_port, "OTLP gRPC receiver started");

        // Start HTTP OTLP receiver
        let http_store = Arc::clone(&self.store);
        let http_tx = self.events_tx.clone();
        let http_port = self.http_port;
        let http_cancel = cancel.clone();
        tokio::spawn(async move {
            if let Err(e) =
                receiver_http::start_http_otlp_server(http_port, http_store, http_tx, http_cancel)
                    .await
            {
                warn!(error = %e, "OTLP HTTP server failed");
            }
        });
        debug!(port = self.http_port, "OTLP HTTP receiver started");

        // Background sweeper for expired telemetry
        let sweep_store = Arc::clone(&self.store);
        let sweep_cancel = cancel.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(30)) => {
                        let mut store = sweep_store.write().await;
                        store.sweep_expired();
                    }
                    _ = sweep_cancel.cancelled() => break,
                }
            }
        });

        Ok(())
    }
}
