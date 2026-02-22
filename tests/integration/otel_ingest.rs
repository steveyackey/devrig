#![cfg(feature = "integration")]

use std::time::Duration;

use devrig::config::model::OtelConfig;
use devrig::dashboard::server::start_dashboard_server;
use devrig::otel::query::SystemStatus;
use devrig::otel::OtelCollector;

use prost::Message;
use tokio_util::sync::CancellationToken;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::metrics::v1::{
    Gauge, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};

fn make_resource(service_name: &str) -> Resource {
    Resource {
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
    }
}

fn now_nanos() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn build_trace_request(service_name: &str) -> ExportTraceServiceRequest {
    let nanos = now_nanos();
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(make_resource(service_name)),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![Span {
                    trace_id: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                    span_id: vec![1, 2, 3, 4, 5, 6, 7, 8],
                    parent_span_id: vec![],
                    name: "test-operation".to_string(),
                    kind: 2,
                    start_time_unix_nano: nanos - 50_000_000,
                    end_time_unix_nano: nanos,
                    attributes: vec![],
                    status: Some(Status {
                        code: 1,
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

fn build_logs_request(service_name: &str) -> ExportLogsServiceRequest {
    let nanos = now_nanos();
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(make_resource(service_name)),
            scope_logs: vec![ScopeLogs {
                scope: None,
                log_records: vec![LogRecord {
                    time_unix_nano: nanos,
                    observed_time_unix_nano: nanos,
                    severity_number: 9, // INFO
                    severity_text: "INFO".to_string(),
                    body: Some(AnyValue {
                        value: Some(
                            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
                                "test log message".to_string(),
                            ),
                        ),
                    }),
                    attributes: vec![],
                    ..Default::default()
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

fn build_metrics_request(service_name: &str) -> ExportMetricsServiceRequest {
    let nanos = now_nanos();
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(make_resource(service_name)),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "test.metric".to_string(),
                    description: "A test metric".to_string(),
                    unit: "ms".to_string(),
                    data: Some(
                        opentelemetry_proto::tonic::metrics::v1::metric::Data::Gauge(Gauge {
                            data_points: vec![NumberDataPoint {
                                time_unix_nano: nanos,
                                start_time_unix_nano: nanos - 1_000_000_000,
                                attributes: vec![],
                                exemplars: vec![],
                                flags: 0,
                                value: Some(
                                    opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsDouble(42.5),
                                ),
                            }],
                        }),
                    ),
                    metadata: vec![],
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

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

    tokio::time::sleep(Duration::from_millis(500)).await;
    cancel
}

// ---------------------------------------------------------------------------
// 3.1: OTLP span ingest
// ---------------------------------------------------------------------------

#[tokio::test]
async fn otel_ingest_spans() {
    let cancel = start_stack(16317, 16318, 16500).await;

    let req = build_trace_request("span-test-svc");
    let body = req.encode_to_vec();

    let client = reqwest::Client::new();
    let resp = client
        .post("http://127.0.0.1:16318/v1/traces")
        .header("Content-Type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .expect("POST /v1/traces should succeed");
    assert_eq!(resp.status(), 200);

    tokio::time::sleep(Duration::from_millis(100)).await;

    let status: SystemStatus = client
        .get("http://127.0.0.1:16500/api/status")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status.span_count, 1, "should have ingested 1 span");
    assert_eq!(status.trace_count, 1, "should have 1 trace");
    assert!(status.services.contains(&"span-test-svc".to_string()));

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// 3.2: OTLP log ingest
// ---------------------------------------------------------------------------

#[tokio::test]
async fn otel_ingest_logs() {
    let cancel = start_stack(16417, 16418, 16600).await;

    let req = build_logs_request("log-test-svc");
    let body = req.encode_to_vec();

    let client = reqwest::Client::new();
    let resp = client
        .post("http://127.0.0.1:16418/v1/logs")
        .header("Content-Type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .expect("POST /v1/logs should succeed");
    assert_eq!(resp.status(), 200);

    tokio::time::sleep(Duration::from_millis(100)).await;

    let status: SystemStatus = client
        .get("http://127.0.0.1:16600/api/status")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status.log_count, 1, "should have ingested 1 log");

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// 3.3: OTLP metric ingest
// ---------------------------------------------------------------------------

#[tokio::test]
async fn otel_ingest_metrics() {
    let cancel = start_stack(16517, 16518, 16700).await;

    let req = build_metrics_request("metric-test-svc");
    let body = req.encode_to_vec();

    let client = reqwest::Client::new();
    let resp = client
        .post("http://127.0.0.1:16518/v1/metrics")
        .header("Content-Type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .expect("POST /v1/metrics should succeed");
    assert_eq!(resp.status(), 200);

    tokio::time::sleep(Duration::from_millis(100)).await;

    let status: SystemStatus = client
        .get("http://127.0.0.1:16700/api/status")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status.metric_count, 1, "should have ingested 1 metric");

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// 3.4: OTEL environment injection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn otel_env_injection() {
    use devrig::config::model::{DashboardConfig, DevrigConfig, OtelConfig, ProjectConfig};
    use devrig::discovery::env::build_service_env;
    use std::collections::{BTreeMap, HashMap};

    let config = DevrigConfig {
        project: ProjectConfig {
            name: "test".to_string(),
        },
        services: {
            let mut m = BTreeMap::new();
            m.insert(
                "api".to_string(),
                devrig::config::model::ServiceConfig {
                    path: None,
                    command: "cargo run".to_string(),
                    port: Some(devrig::config::model::Port::Fixed(3000)),
                    env: BTreeMap::new(),
                    depends_on: Vec::new(),
                    restart: None,
                },
            );
            m
        },
        infra: BTreeMap::new(),
        compose: None,
        cluster: None,
        dashboard: Some(DashboardConfig {
            port: 4000,
            enabled: Some(true),
            otel: Some(OtelConfig {
                grpc_port: 4317,
                http_port: 4318,
                trace_buffer: 10000,
                metric_buffer: 10000,
                log_buffer: 10000,
                retention: "1h".to_string(),
            }),
        }),
        env: BTreeMap::new(),
        network: None,
    };

    let mut ports = HashMap::new();
    ports.insert("service:api".to_string(), 3000u16);

    let env = build_service_env("api", &config, &ports);

    assert!(
        env.contains_key("OTEL_EXPORTER_OTLP_ENDPOINT"),
        "OTEL_EXPORTER_OTLP_ENDPOINT should be set when dashboard is enabled"
    );
    assert_eq!(env["OTEL_EXPORTER_OTLP_ENDPOINT"], "http://localhost:4318");
    assert!(
        env.contains_key("OTEL_SERVICE_NAME"),
        "OTEL_SERVICE_NAME should be set"
    );
    assert_eq!(env["OTEL_SERVICE_NAME"], "api");
}
