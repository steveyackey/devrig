use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_server::{
    LogsService, LogsServiceServer,
};
use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_server::{
    MetricsService, MetricsServiceServer,
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::{
    TraceService, TraceServiceServer,
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
struct OtlpGrpcReceiver {
    store: Arc<RwLock<TelemetryStore>>,
    events_tx: broadcast::Sender<TelemetryEvent>,
}

#[tonic::async_trait]
impl TraceService for OtlpGrpcReceiver {
    async fn export(
        &self,
        request: tonic::Request<ExportTraceServiceRequest>,
    ) -> Result<tonic::Response<ExportTraceServiceResponse>, tonic::Status> {
        let req = request.into_inner();
        let mut events = Vec::new();

        {
            let mut store = self.store.write().await;
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

        // Send events outside the write lock
        for event in events {
            let _ = self.events_tx.send(event);
        }

        Ok(tonic::Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}

#[tonic::async_trait]
impl MetricsService for OtlpGrpcReceiver {
    async fn export(
        &self,
        request: tonic::Request<ExportMetricsServiceRequest>,
    ) -> Result<tonic::Response<ExportMetricsServiceResponse>, tonic::Status> {
        let req = request.into_inner();
        let mut events = Vec::new();

        {
            let mut store = self.store.write().await;
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
            let _ = self.events_tx.send(event);
        }

        Ok(tonic::Response::new(ExportMetricsServiceResponse {
            partial_success: None,
        }))
    }
}

#[tonic::async_trait]
impl LogsService for OtlpGrpcReceiver {
    async fn export(
        &self,
        request: tonic::Request<ExportLogsServiceRequest>,
    ) -> Result<tonic::Response<ExportLogsServiceResponse>, tonic::Status> {
        let req = request.into_inner();
        let mut events = Vec::new();

        {
            let mut store = self.store.write().await;
            for resource_logs in &req.resource_logs {
                let service_name = resource_logs
                    .resource
                    .as_ref()
                    .map(|r| extract_service_name(&r.attributes))
                    .unwrap_or_else(|| "unknown".to_string());

                for scope_logs in &resource_logs.scope_logs {
                    for log_record in &scope_logs.log_records {
                        let mut stored = proto_log_to_stored(log_record, &service_name);
                        stored.attributes.push(("log.source".to_string(), "otlp".to_string()));
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
            let _ = self.events_tx.send(event);
        }

        Ok(tonic::Response::new(ExportLogsServiceResponse {
            partial_success: None,
        }))
    }
}

pub async fn start_grpc_server(
    port: u16,
    store: Arc<RwLock<TelemetryStore>>,
    events_tx: broadcast::Sender<TelemetryEvent>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let receiver = OtlpGrpcReceiver { store, events_tx };

    let addr = format!("0.0.0.0:{}", port).parse()?;

    tonic::transport::Server::builder()
        .add_service(TraceServiceServer::new(receiver.clone()))
        .add_service(MetricsServiceServer::new(receiver.clone()))
        .add_service(LogsServiceServer::new(receiver))
        .serve_with_shutdown(addr, cancel.cancelled_owned())
        .await?;

    Ok(())
}
