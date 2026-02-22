use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------
// Span types
// -----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanStatus {
    Ok,
    Error,
    Unset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanKind {
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSpan {
    pub record_id: u64,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service_name: String,
    pub operation_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_ms: u64,
    pub status: SpanStatus,
    pub status_message: Option<String>,
    pub attributes: Vec<(String, String)>,
    pub kind: SpanKind,
}

// -----------------------------------------------------------------------
// Log types
// -----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum LogSeverity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogSeverity {
    /// Convert from OTLP severity number (1-24).
    pub fn from_severity_number(n: i32) -> Self {
        match n {
            1..=4 => LogSeverity::Trace,
            5..=8 => LogSeverity::Debug,
            9..=12 => LogSeverity::Info,
            13..=16 => LogSeverity::Warn,
            17..=20 => LogSeverity::Error,
            21..=24 => LogSeverity::Fatal,
            _ => LogSeverity::Info,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredLog {
    pub record_id: u64,
    pub timestamp: DateTime<Utc>,
    pub service_name: String,
    pub severity: LogSeverity,
    pub body: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub attributes: Vec<(String, String)>,
}

// -----------------------------------------------------------------------
// Metric types
// -----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricType {
    Gauge,
    Counter,
    Histogram,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMetric {
    pub record_id: u64,
    pub timestamp: DateTime<Utc>,
    pub service_name: String,
    pub metric_name: String,
    pub metric_type: MetricType,
    pub value: f64,
    pub attributes: Vec<(String, String)>,
    pub unit: Option<String>,
}

// -----------------------------------------------------------------------
// WebSocket event types
// -----------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum TelemetryEvent {
    TraceUpdate {
        trace_id: String,
        service: String,
        duration_ms: u64,
        has_error: bool,
    },
    LogRecord {
        trace_id: Option<String>,
        severity: String,
        body: String,
        service: String,
    },
    MetricUpdate {
        name: String,
        value: f64,
        service: String,
    },
    ServiceStatusChange {
        service: String,
        status: String,
    },
}

// -----------------------------------------------------------------------
// Proto conversion helpers
// -----------------------------------------------------------------------

/// Extract `service.name` from resource attributes.
pub fn extract_service_name(
    attributes: &[opentelemetry_proto::tonic::common::v1::KeyValue],
) -> String {
    for kv in attributes {
        if kv.key == "service.name" {
            if let Some(ref v) = kv.value {
                if let Some(ref val) = v.value {
                    return match val {
                        opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
                            s,
                        ) => s.clone(),
                        _ => "unknown".to_string(),
                    };
                }
            }
        }
    }
    "unknown".to_string()
}

/// Convert OTLP attributes to simple key-value string pairs (first N).
pub fn convert_attributes(
    attrs: &[opentelemetry_proto::tonic::common::v1::KeyValue],
    max: usize,
) -> Vec<(String, String)> {
    attrs
        .iter()
        .take(max)
        .map(|kv| {
            let val = kv
                .value
                .as_ref()
                .and_then(|v| v.value.as_ref())
                .map(|v| match v {
                    opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(s) => {
                        s.clone()
                    }
                    opentelemetry_proto::tonic::common::v1::any_value::Value::IntValue(i) => {
                        i.to_string()
                    }
                    opentelemetry_proto::tonic::common::v1::any_value::Value::DoubleValue(d) => {
                        d.to_string()
                    }
                    opentelemetry_proto::tonic::common::v1::any_value::Value::BoolValue(b) => {
                        b.to_string()
                    }
                    _ => "<complex>".to_string(),
                })
                .unwrap_or_default();
            (kv.key.clone(), val)
        })
        .collect()
}

/// Convert nanosecond timestamp to DateTime<Utc>.
pub fn nanos_to_datetime(nanos: u64) -> DateTime<Utc> {
    let secs = (nanos / 1_000_000_000) as i64;
    let nsecs = (nanos % 1_000_000_000) as u32;
    DateTime::from_timestamp(secs, nsecs).unwrap_or_else(Utc::now)
}

/// Convert a proto span to a StoredSpan.
pub fn proto_span_to_stored(
    span: &opentelemetry_proto::tonic::trace::v1::Span,
    service_name: &str,
) -> StoredSpan {
    let trace_id = hex::encode(&span.trace_id);
    let span_id = hex::encode(&span.span_id);
    let parent_span_id = if span.parent_span_id.is_empty() {
        None
    } else {
        Some(hex::encode(&span.parent_span_id))
    };

    let start_time = nanos_to_datetime(span.start_time_unix_nano);
    let end_time = nanos_to_datetime(span.end_time_unix_nano);
    let duration_ms = span
        .end_time_unix_nano
        .saturating_sub(span.start_time_unix_nano)
        / 1_000_000;

    let status = span
        .status
        .as_ref()
        .map(|s| match s.code {
            0 => SpanStatus::Unset,
            1 => SpanStatus::Ok,
            2 => SpanStatus::Error,
            _ => SpanStatus::Unset,
        })
        .unwrap_or(SpanStatus::Unset);

    let status_message = span.status.as_ref().and_then(|s| {
        if s.message.is_empty() {
            None
        } else {
            Some(s.message.clone())
        }
    });

    let kind = match span.kind {
        1 => SpanKind::Internal,
        2 => SpanKind::Server,
        3 => SpanKind::Client,
        4 => SpanKind::Producer,
        5 => SpanKind::Consumer,
        _ => SpanKind::Internal,
    };

    StoredSpan {
        record_id: 0, // assigned by store
        trace_id,
        span_id,
        parent_span_id,
        service_name: service_name.to_string(),
        operation_name: span.name.clone(),
        start_time,
        end_time,
        duration_ms,
        status,
        status_message,
        attributes: convert_attributes(&span.attributes, 20),
        kind,
    }
}

/// Convert a proto log record to a StoredLog.
pub fn proto_log_to_stored(
    log: &opentelemetry_proto::tonic::logs::v1::LogRecord,
    service_name: &str,
) -> StoredLog {
    let timestamp = if log.time_unix_nano > 0 {
        nanos_to_datetime(log.time_unix_nano)
    } else if log.observed_time_unix_nano > 0 {
        nanos_to_datetime(log.observed_time_unix_nano)
    } else {
        Utc::now()
    };

    let severity = LogSeverity::from_severity_number(log.severity_number);

    let body = log
        .body
        .as_ref()
        .and_then(|v| v.value.as_ref())
        .map(|v| match v {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(s) => s.clone(),
            _ => format!("{:?}", v),
        })
        .unwrap_or_default();

    let trace_id = if log.trace_id.is_empty() {
        None
    } else {
        Some(hex::encode(&log.trace_id))
    };

    let span_id = if log.span_id.is_empty() {
        None
    } else {
        Some(hex::encode(&log.span_id))
    };

    StoredLog {
        record_id: 0,
        timestamp,
        service_name: service_name.to_string(),
        severity,
        body,
        trace_id,
        span_id,
        attributes: convert_attributes(&log.attributes, 20),
    }
}

/// Convert proto metric data points to StoredMetric entries.
pub fn proto_metrics_to_stored(
    metric: &opentelemetry_proto::tonic::metrics::v1::Metric,
    service_name: &str,
) -> Vec<StoredMetric> {
    let mut results = Vec::new();
    let name = metric.name.clone();
    let unit = if metric.unit.is_empty() {
        None
    } else {
        Some(metric.unit.clone())
    };

    if let Some(ref data) = metric.data {
        use opentelemetry_proto::tonic::metrics::v1::metric::Data;
        match data {
            Data::Gauge(gauge) => {
                for dp in &gauge.data_points {
                    let value = extract_number_value(dp);
                    let timestamp = nanos_to_datetime(dp.time_unix_nano);
                    results.push(StoredMetric {
                        record_id: 0,
                        timestamp,
                        service_name: service_name.to_string(),
                        metric_name: name.clone(),
                        metric_type: MetricType::Gauge,
                        value,
                        attributes: convert_attributes(&dp.attributes, 20),
                        unit: unit.clone(),
                    });
                }
            }
            Data::Sum(sum) => {
                for dp in &sum.data_points {
                    let value = extract_number_value(dp);
                    let timestamp = nanos_to_datetime(dp.time_unix_nano);
                    results.push(StoredMetric {
                        record_id: 0,
                        timestamp,
                        service_name: service_name.to_string(),
                        metric_name: name.clone(),
                        metric_type: MetricType::Counter,
                        value,
                        attributes: convert_attributes(&dp.attributes, 20),
                        unit: unit.clone(),
                    });
                }
            }
            Data::Histogram(hist) => {
                for dp in &hist.data_points {
                    let value = dp.sum.unwrap_or(0.0);
                    let timestamp = nanos_to_datetime(dp.time_unix_nano);
                    results.push(StoredMetric {
                        record_id: 0,
                        timestamp,
                        service_name: service_name.to_string(),
                        metric_name: name.clone(),
                        metric_type: MetricType::Histogram,
                        value,
                        attributes: convert_attributes(&dp.attributes, 20),
                        unit: unit.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    results
}

fn extract_number_value(dp: &opentelemetry_proto::tonic::metrics::v1::NumberDataPoint) -> f64 {
    dp.value
        .as_ref()
        .map(|v| {
            use opentelemetry_proto::tonic::metrics::v1::number_data_point::Value;
            match v {
                Value::AsDouble(d) => *d,
                Value::AsInt(i) => *i as f64,
            }
        })
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_from_number() {
        assert_eq!(LogSeverity::from_severity_number(1), LogSeverity::Trace);
        assert_eq!(LogSeverity::from_severity_number(5), LogSeverity::Debug);
        assert_eq!(LogSeverity::from_severity_number(9), LogSeverity::Info);
        assert_eq!(LogSeverity::from_severity_number(13), LogSeverity::Warn);
        assert_eq!(LogSeverity::from_severity_number(17), LogSeverity::Error);
        assert_eq!(LogSeverity::from_severity_number(21), LogSeverity::Fatal);
    }

    #[test]
    fn nanos_to_datetime_conversion() {
        let dt = nanos_to_datetime(1_700_000_000_000_000_000);
        assert_eq!(dt.timestamp(), 1_700_000_000);
    }

    #[test]
    fn hex_encode_trace_id() {
        let bytes: Vec<u8> = vec![
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10,
        ];
        assert_eq!(hex::encode(&bytes), "0102030405060708090a0b0c0d0e0f10");
    }
}
