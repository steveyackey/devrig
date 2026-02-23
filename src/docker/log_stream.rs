use std::sync::Arc;

use bollard::query_parameters::LogsOptions;
use bollard::Docker;
use chrono::Utc;
use futures_util::StreamExt;
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::warn;

use crate::otel::storage::TelemetryStore;
use crate::otel::types::{LogSeverity, StoredLog, TelemetryEvent};
use crate::ui::logs::detect_log_level;

/// Spawn a background task that streams Docker container logs into the
/// TelemetryStore and broadcasts them over WebSocket.
///
/// Only captures new logs produced after the stream starts (`since: now`).
/// The task exits cleanly when `cancel` fires or the container stops.
pub fn spawn_docker_log_stream(
    docker: Docker,
    container_id: String,
    service_name: String,
    store: Arc<RwLock<TelemetryStore>>,
    events_tx: broadcast::Sender<TelemetryEvent>,
    cancel: CancellationToken,
    tracker: &TaskTracker,
) {
    let svc = service_name.clone();
    tracker.spawn(async move {
        let since = Utc::now().timestamp() as i32;
        let options = LogsOptions {
            follow: true,
            stdout: true,
            stderr: true,
            since,
            ..Default::default()
        };

        let mut stream = docker.logs(&container_id, Some(options));

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                item = stream.next() => {
                    match item {
                        Some(Ok(output)) => {
                            use bollard::container::LogOutput;
                            let (text, is_stderr) = match &output {
                                LogOutput::StdOut { message } => {
                                    (String::from_utf8_lossy(message).to_string(), false)
                                }
                                LogOutput::StdErr { message } => {
                                    (String::from_utf8_lossy(message).to_string(), true)
                                }
                                _ => continue,
                            };

                            let text = text.trim_end().to_string();
                            if text.is_empty() {
                                continue;
                            }

                            let level = detect_log_level(&text);
                            let severity = LogSeverity::from_log_level(level, is_stderr);

                            let stored = StoredLog {
                                record_id: 0,
                                timestamp: Utc::now(),
                                service_name: svc.clone(),
                                severity,
                                body: text.clone(),
                                trace_id: None,
                                span_id: None,
                                attributes: vec![
                                    ("log.source".to_string(), "docker".to_string()),
                                ],
                            };

                            let event = TelemetryEvent::LogRecord {
                                trace_id: None,
                                severity: format!("{:?}", stored.severity),
                                body: stored.body.clone(),
                                service: stored.service_name.clone(),
                            };

                            { store.write().await.insert_log(stored); }
                            let _ = events_tx.send(event);
                        }
                        Some(Err(e)) => {
                            warn!(
                                service = %svc,
                                error = %e,
                                "docker log stream error"
                            );
                            break;
                        }
                        None => break, // container stopped or stream ended
                    }
                }
            }
        }
    });
}
