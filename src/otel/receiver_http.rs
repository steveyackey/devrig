use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use prost::Message;
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;

use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};

use super::storage::TelemetryStore;
use super::types::{
    extract_service_name, proto_log_to_stored, proto_metrics_to_stored, proto_span_to_stored,
    TelemetryEvent,
};

#[derive(Clone)]
struct HttpOtlpState {
    store: Arc<RwLock<TelemetryStore>>,
    events_tx: broadcast::Sender<TelemetryEvent>,
}

async fn post_traces(
    State(state): State<HttpOtlpState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let req = match decode_request::<ExportTraceServiceRequest>(&headers, &body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };

    let mut events = Vec::new();

    {
        let mut store = state.store.write().await;
        for resource_spans in &req.resource_spans {
            let service_name = resource_spans
                .resource
                .as_ref()
                .map(|r| extract_service_name(&r.attributes))
                .unwrap_or_else(|| "unknown".to_string());

            for scope_spans in &resource_spans.scope_spans {
                for span in &scope_spans.spans {
                    let stored = proto_span_to_stored(span, &service_name);
                    let has_error = stored.status == super::types::SpanStatus::Error;
                    events.push(TelemetryEvent::TraceUpdate {
                        trace_id: stored.trace_id.clone(),
                        service: stored.service_name.clone(),
                        duration_ms: stored.duration_ms,
                        has_error,
                    });
                    store.insert_span(stored);
                }
            }
        }
    }

    for event in events {
        let _ = state.events_tx.send(event);
    }

    let resp = ExportTraceServiceResponse {
        partial_success: None,
    };
    encode_response(&resp)
}

async fn post_metrics(
    State(state): State<HttpOtlpState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let req = match decode_request::<ExportMetricsServiceRequest>(&headers, &body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };

    let mut events = Vec::new();

    {
        let mut store = state.store.write().await;
        for resource_metrics in &req.resource_metrics {
            let service_name = resource_metrics
                .resource
                .as_ref()
                .map(|r| extract_service_name(&r.attributes))
                .unwrap_or_else(|| "unknown".to_string());

            for scope_metrics in &resource_metrics.scope_metrics {
                for metric in &scope_metrics.metrics {
                    let stored_metrics = proto_metrics_to_stored(metric, &service_name);
                    for stored in stored_metrics {
                        events.push(TelemetryEvent::MetricUpdate {
                            name: stored.metric_name.clone(),
                            value: stored.value,
                            service: stored.service_name.clone(),
                        });
                        store.insert_metric(stored);
                    }
                }
            }
        }
    }

    for event in events {
        let _ = state.events_tx.send(event);
    }

    let resp = ExportMetricsServiceResponse {
        partial_success: None,
    };
    encode_response(&resp)
}

async fn post_logs(
    State(state): State<HttpOtlpState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let req = match decode_request::<ExportLogsServiceRequest>(&headers, &body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };

    let mut events = Vec::new();

    {
        let mut store = state.store.write().await;
        for resource_logs in &req.resource_logs {
            let service_name = resource_logs
                .resource
                .as_ref()
                .map(|r| extract_service_name(&r.attributes))
                .unwrap_or_else(|| "unknown".to_string());

            for scope_logs in &resource_logs.scope_logs {
                for log_record in &scope_logs.log_records {
                    let stored = proto_log_to_stored(log_record, &service_name);
                    events.push(TelemetryEvent::LogRecord {
                        trace_id: stored.trace_id.clone(),
                        severity: format!("{:?}", stored.severity),
                        body: stored.body.clone(),
                        service: stored.service_name.clone(),
                    });
                    store.insert_log(stored);
                }
            }
        }
    }

    for event in events {
        let _ = state.events_tx.send(event);
    }

    let resp = ExportLogsServiceResponse {
        partial_success: None,
    };
    encode_response(&resp)
}

fn decode_request<T: Message + Default + serde::de::DeserializeOwned>(
    headers: &axum::http::HeaderMap,
    body: &[u8],
) -> Result<T, String> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/x-protobuf");

    if content_type.contains("protobuf") || content_type.contains("proto") {
        T::decode(body).map_err(|e| format!("protobuf decode error: {}", e))
    } else if content_type.contains("json") {
        // For JSON, try to use serde if available via the with-serde feature
        serde_json::from_slice(body).map_err(|e| format!("JSON decode error: {}", e))
    } else {
        // Default to protobuf
        T::decode(body).map_err(|e| format!("decode error: {}", e))
    }
}

fn encode_response<T: Message>(msg: &T) -> axum::response::Response {
    let bytes = msg.encode_to_vec();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/x-protobuf")],
        bytes,
    )
        .into_response()
}

pub fn otlp_http_router(
    store: Arc<RwLock<TelemetryStore>>,
    events_tx: broadcast::Sender<TelemetryEvent>,
) -> Router {
    let state = HttpOtlpState { store, events_tx };

    Router::new()
        .route("/v1/traces", post(post_traces))
        .route("/v1/metrics", post(post_metrics))
        .route("/v1/logs", post(post_logs))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn start_http_otlp_server(
    port: u16,
    store: Arc<RwLock<TelemetryStore>>,
    events_tx: broadcast::Sender<TelemetryEvent>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let app = otlp_http_router(store, events_tx);
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await?;

    Ok(())
}
