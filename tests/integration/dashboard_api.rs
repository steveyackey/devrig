#![cfg(feature = "integration")]

use std::time::Duration;

use devrig::config::model::OtelConfig;
use devrig::dashboard::server::start_dashboard_server;
use devrig::otel::query::{SystemStatus, TraceSummary};
use devrig::otel::OtelCollector;

use prost::Message;
use tokio_util::sync::CancellationToken;

use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};

/// Build an OTLP ExportTraceServiceRequest with a single span.
fn build_trace_request(
    trace_id: &[u8; 16],
    span_id: &[u8; 8],
    service_name: &str,
    operation: &str,
) -> ExportTraceServiceRequest {
    let now_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "service.name".to_string(),
                    value: Some(AnyValue {
                        value: Some(
                            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
                                service_name.to_string(),
                            ),
                        ),
                    }),
                }],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![Span {
                    trace_id: trace_id.to_vec(),
                    span_id: span_id.to_vec(),
                    parent_span_id: vec![],
                    name: operation.to_string(),
                    kind: 2, // Server
                    start_time_unix_nano: now_nanos - 50_000_000, // 50ms ago
                    end_time_unix_nano: now_nanos,
                    attributes: vec![KeyValue {
                        key: "http.method".to_string(),
                        value: Some(AnyValue {
                            value: Some(
                                opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
                                    "GET".to_string(),
                                ),
                            ),
                        }),
                    }],
                    status: Some(Status {
                        code: 1, // Ok
                        message: String::new(),
                    }),
                    ..Default::default()
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

/// Helper: start the OTel collector and dashboard on the given ports, returning
/// a CancellationToken that callers should cancel to shut everything down.
async fn start_stack(grpc_port: u16, http_port: u16, dashboard_port: u16) -> CancellationToken {
    let cancel = CancellationToken::new();

    let otel_config = OtelConfig {
        grpc_port,
        http_port,
        trace_buffer: 100,
        metric_buffer: 100,
        log_buffer: 100,
        retention: "1h".to_string(),
    };

    let collector = OtelCollector::new(&otel_config);
    collector.start(cancel.clone()).await.unwrap();

    let store = collector.store();
    let events_tx = collector.events_tx();

    let dash_cancel = cancel.clone();
    tokio::spawn(async move {
        let _ = start_dashboard_server(dashboard_port, store, events_tx, dash_cancel, None).await;
    });

    // Give servers a moment to bind their ports.
    tokio::time::sleep(Duration::from_millis(500)).await;

    cancel
}

// ---------------------------------------------------------------------------
// Test 1: Status endpoint returns empty counts when no data has been ingested.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dashboard_status_endpoint_returns_empty_initially() {
    let cancel = start_stack(15317, 15318, 15500).await;

    let client = reqwest::Client::new();
    let resp = client
        .get("http://127.0.0.1:15500/api/status")
        .send()
        .await
        .expect("GET /api/status should succeed");

    assert_eq!(resp.status(), 200);

    let status: SystemStatus = resp.json().await.expect("response should be valid JSON");
    assert_eq!(status.span_count, 0, "no spans should exist yet");
    assert_eq!(status.log_count, 0, "no logs should exist yet");
    assert_eq!(status.metric_count, 0, "no metrics should exist yet");
    assert_eq!(status.trace_count, 0, "no traces should exist yet");
    assert!(
        status.services.is_empty(),
        "no services should be registered yet"
    );

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// Test 2: The /api/traces and /api/logs and /api/metrics endpoints all return
//         empty arrays when nothing has been ingested.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dashboard_list_endpoints_return_empty() {
    let cancel = start_stack(15417, 15418, 15600).await;

    let client = reqwest::Client::new();

    // /api/traces
    let resp = client
        .get("http://127.0.0.1:15600/api/traces")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let traces: Vec<TraceSummary> = resp.json().await.unwrap();
    assert!(traces.is_empty(), "/api/traces should return an empty list");

    // /api/logs
    let resp = client
        .get("http://127.0.0.1:15600/api/logs")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let logs: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(logs.is_empty(), "/api/logs should return an empty list");

    // /api/metrics
    let resp = client
        .get("http://127.0.0.1:15600/api/metrics")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let metrics: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(
        metrics.is_empty(),
        "/api/metrics should return an empty list"
    );

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// Test 3: Full round-trip -- ingest a span via HTTP OTLP then query it back
//         through the dashboard REST API.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn otlp_http_ingestion_and_dashboard_query() {
    let cancel = start_stack(15517, 15518, 15700).await;

    let trace_id: [u8; 16] = [
        0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
        0x89,
    ];
    let span_id: [u8; 8] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];

    let req = build_trace_request(&trace_id, &span_id, "test-service", "GET /health");
    let body = req.encode_to_vec();

    // POST the span to the HTTP OTLP receiver.
    let client = reqwest::Client::new();
    let ingest_resp = client
        .post("http://127.0.0.1:15518/v1/traces")
        .header("Content-Type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .expect("POST /v1/traces should succeed");
    assert_eq!(
        ingest_resp.status(),
        200,
        "OTLP HTTP ingestion should return 200"
    );

    // Small delay so the store is updated before we query.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify /api/status now reports 1 span and 1 trace.
    let status: SystemStatus = client
        .get("http://127.0.0.1:15700/api/status")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status.span_count, 1, "should have exactly 1 span");
    assert_eq!(status.trace_count, 1, "should have exactly 1 trace");
    assert!(
        status.services.contains(&"test-service".to_string()),
        "services list should contain test-service"
    );

    // Verify /api/traces returns the trace summary.
    let traces: Vec<TraceSummary> = client
        .get("http://127.0.0.1:15700/api/traces")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(traces.len(), 1, "should have 1 trace summary");

    let expected_trace_id = hex::encode(trace_id);
    assert_eq!(traces[0].trace_id, expected_trace_id);
    assert_eq!(traces[0].root_operation, "GET /health");
    assert_eq!(traces[0].span_count, 1);
    assert!(!traces[0].has_error);
    assert!(traces[0].services.contains(&"test-service".to_string()));

    // Verify /api/traces/:trace_id returns the detailed trace.
    let detail_resp = client
        .get(format!(
            "http://127.0.0.1:15700/api/traces/{}",
            expected_trace_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(detail_resp.status(), 200);

    let detail: serde_json::Value = detail_resp.json().await.unwrap();
    assert_eq!(detail["trace_id"], expected_trace_id);

    let spans = detail["spans"]
        .as_array()
        .expect("spans should be an array");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0]["operation_name"], "GET /health");
    assert_eq!(spans[0]["service_name"], "test-service");
    assert_eq!(spans[0]["span_id"], hex::encode(span_id));

    // Verify /api/traces/:nonexistent returns 404.
    let missing_resp = client
        .get("http://127.0.0.1:15700/api/traces/0000000000000000deadbeefdeadbeef")
        .send()
        .await
        .unwrap();
    assert_eq!(
        missing_resp.status(),
        404,
        "non-existent trace should return 404"
    );

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// Test 4: WebSocket connection receives a live event when a span is ingested.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn websocket_receives_trace_update_event() {
    let cancel = start_stack(15617, 15618, 15800).await;

    // Connect a WebSocket client to the dashboard.
    let (mut ws_stream, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:15800/ws")
        .await
        .expect("WebSocket handshake should succeed");

    // Now ingest a span so a TraceUpdate event is broadcast.
    let trace_id: [u8; 16] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10,
    ];
    let span_id: [u8; 8] = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11];

    let req = build_trace_request(&trace_id, &span_id, "ws-test-svc", "POST /data");
    let body = req.encode_to_vec();

    let client = reqwest::Client::new();
    let ingest_resp = client
        .post("http://127.0.0.1:15618/v1/traces")
        .header("Content-Type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(ingest_resp.status(), 200);

    // Read the next message from the WebSocket -- it should be the TraceUpdate event.
    use futures_util::StreamExt;

    let msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .expect("should receive a WS message within 5 seconds")
        .expect("stream should not be closed")
        .expect("message should not be an error");

    let text = msg.into_text().expect("message should be text");
    let event: serde_json::Value =
        serde_json::from_str(&text).expect("message should be valid JSON");

    assert_eq!(
        event["type"], "TraceUpdate",
        "event type should be TraceUpdate"
    );
    let payload = &event["payload"];
    assert_eq!(payload["trace_id"], hex::encode(trace_id));
    assert_eq!(payload["service"], "ws-test-svc");
    assert_eq!(payload["has_error"], false);

    // Clean close -- drop the stream.
    drop(ws_stream);

    cancel.cancel();
}
