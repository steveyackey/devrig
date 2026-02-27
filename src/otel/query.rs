use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::storage::TelemetryStore;
use super::types::{LogSeverity, MetricType, SpanStatus, StoredLog, StoredMetric, StoredSpan};

// -----------------------------------------------------------------------
// Query parameters
// -----------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct TraceQuery {
    pub service: Option<String>,
    pub status: Option<String>,
    pub min_duration_ms: Option<u64>,
    pub search: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LogQuery {
    pub service: Option<String>,
    pub severity: Option<String>,
    pub search: Option<String>,
    pub trace_id: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    /// Filter by log source: "process" (stdout+stderr), "stdout", "stderr", "docker", "otlp", or omit for all.
    pub source: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MetricQuery {
    pub name: Option<String>,
    pub metric_type: Option<String>,
    pub service: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MetricSeriesQuery {
    pub name: String,
    pub service: Option<String>,
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricSeriesPoint {
    pub t: i64,  // unix milliseconds
    pub v: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricSeries {
    pub metric_name: String,
    pub service_name: String,
    pub metric_type: MetricType,
    pub unit: Option<String>,
    pub points: Vec<MetricSeriesPoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricSeriesResponse {
    pub series: Vec<MetricSeries>,
}

// -----------------------------------------------------------------------
// Query result types
// -----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSummary {
    pub trace_id: String,
    pub services: Vec<String>,
    pub root_operation: String,
    pub duration_ms: u64,
    pub span_count: usize,
    pub has_error: bool,
    pub start_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceDetail {
    pub trace_id: String,
    pub spans: Vec<StoredSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedTelemetry {
    pub logs: Vec<StoredLog>,
    pub metrics: Vec<StoredMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub span_count: usize,
    pub log_count: usize,
    pub metric_count: usize,
    pub services: Vec<String>,
    pub trace_count: usize,
}

// -----------------------------------------------------------------------
// Query methods on TelemetryStore
// -----------------------------------------------------------------------

impl TelemetryStore {
    /// List traces with optional filters.
    pub fn query_traces(&self, query: &TraceQuery) -> Vec<TraceSummary> {
        let limit = query.limit.unwrap_or(100);

        // Group spans by trace_id
        let mut trace_map: HashMap<&str, Vec<&StoredSpan>> = HashMap::new();
        for span in self.spans() {
            trace_map.entry(&span.trace_id).or_default().push(span);
        }

        let mut summaries: Vec<TraceSummary> = trace_map
            .into_iter()
            .filter_map(|(trace_id, spans)| {
                let span_count = spans.len();
                let mut services: Vec<String> = spans
                    .iter()
                    .map(|s| s.service_name.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                services.sort();

                let has_error = spans.iter().any(|s| s.status == SpanStatus::Error);

                // Find root span (no parent) or earliest span
                let root = spans
                    .iter()
                    .find(|s| s.parent_span_id.is_none())
                    .or_else(|| spans.iter().min_by_key(|s| s.start_time));

                let root_operation = root.map(|s| s.operation_name.clone()).unwrap_or_default();
                let start_time = root.map(|s| s.start_time).unwrap_or_else(Utc::now);

                let duration_ms = spans
                    .iter()
                    .map(|s| {
                        let end_nanos = s.end_time.timestamp_millis();
                        let start_nanos = start_time.timestamp_millis();
                        (end_nanos - start_nanos).max(0) as u64
                    })
                    .max()
                    .unwrap_or(0);

                let summary = TraceSummary {
                    trace_id: trace_id.to_string(),
                    services,
                    root_operation,
                    duration_ms,
                    span_count,
                    has_error,
                    start_time,
                };

                // Apply filters
                if let Some(ref svc) = query.service {
                    if !summary.services.contains(svc) {
                        return None;
                    }
                }

                if let Some(ref status) = query.status {
                    match status.as_str() {
                        "error" if !summary.has_error => return None,
                        "ok" if summary.has_error => return None,
                        _ => {}
                    }
                }

                if let Some(min_dur) = query.min_duration_ms {
                    if summary.duration_ms < min_dur {
                        return None;
                    }
                }

                if let Some(ref search) = query.search {
                    if !summary
                        .root_operation
                        .to_lowercase()
                        .contains(&search.to_lowercase())
                    {
                        return None;
                    }
                }

                if let Some(since) = query.since {
                    if summary.start_time < since {
                        return None;
                    }
                }

                Some(summary)
            })
            .collect();

        // Sort by start_time descending (most recent first)
        summaries.sort_by(|a, b| b.start_time.cmp(&a.start_time));
        summaries.truncate(limit);
        summaries
    }

    /// Get all spans for a specific trace.
    pub fn get_trace(&self, trace_id: &str) -> Option<TraceDetail> {
        let record_ids = self.trace_index().get(trace_id)?;
        let spans: Vec<StoredSpan> = self
            .spans()
            .iter()
            .filter(|s| record_ids.contains(&s.record_id))
            .cloned()
            .collect();

        if spans.is_empty() {
            return None;
        }

        Some(TraceDetail {
            trace_id: trace_id.to_string(),
            spans,
        })
    }

    /// Query logs with optional filters.
    pub fn query_logs(&self, query: &LogQuery) -> Vec<StoredLog> {
        let limit = query.limit.unwrap_or(200);

        let results: Vec<StoredLog> = self
            .logs()
            .iter()
            .rev() // most recent first
            .filter(|log| {
                if let Some(ref svc) = query.service {
                    if &log.service_name != svc {
                        return false;
                    }
                }
                if let Some(ref sev) = query.severity {
                    let target = parse_severity(sev);
                    if log.severity < target {
                        return false;
                    }
                }
                if let Some(ref search) = query.search {
                    if !log.body.to_lowercase().contains(&search.to_lowercase()) {
                        return false;
                    }
                }
                if let Some(ref tid) = query.trace_id {
                    match &log.trace_id {
                        Some(lt) if lt == tid => {}
                        _ => return false,
                    }
                }
                if let Some(since) = query.since {
                    if log.timestamp < since {
                        return false;
                    }
                }
                if let Some(ref src) = query.source {
                    let log_source = log.attributes.iter()
                        .find(|(k, _)| k == "log.source")
                        .map(|(_, v)| v.as_str());
                    match src.as_str() {
                        "process" => if !matches!(log_source, Some("stdout" | "stderr")) { return false; },
                        other => if log_source != Some(other) { return false; },
                    }
                }
                true
            })
            .take(limit)
            .cloned()
            .collect();

        results
    }

    /// Query metrics with optional filters.
    pub fn query_metrics(&self, query: &MetricQuery) -> Vec<StoredMetric> {
        let limit = query.limit.unwrap_or(500);

        let results: Vec<StoredMetric> = self
            .metrics()
            .iter()
            .rev()
            .filter(|m| {
                if let Some(ref name) = query.name {
                    if !m.metric_name.to_lowercase().contains(&name.to_lowercase()) {
                        return false;
                    }
                }
                if let Some(ref mt) = query.metric_type {
                    let type_str = format!("{:?}", m.metric_type);
                    if !type_str.eq_ignore_ascii_case(mt) {
                        return false;
                    }
                }
                if let Some(ref svc) = query.service {
                    if &m.service_name != svc {
                        return false;
                    }
                }
                if let Some(since) = query.since {
                    if m.timestamp < since {
                        return false;
                    }
                }
                true
            })
            .take(limit)
            .cloned()
            .collect();

        results
    }

    /// Get system status summary.
    pub fn get_status(&self) -> SystemStatus {
        SystemStatus {
            span_count: self.get_span_count(),
            log_count: self.get_log_count(),
            metric_count: self.get_metric_count(),
            services: self.service_names(),
            trace_count: self.trace_index().len(),
        }
    }

    /// Query metric time-series grouped by metric_name + service_name.
    pub fn query_metric_series(&self, query: &MetricSeriesQuery) -> MetricSeriesResponse {
        let since = query.since.unwrap_or_else(|| Utc::now() - chrono::Duration::minutes(5));

        // Group metrics by (metric_name, service_name)
        let mut groups: HashMap<(String, String), Vec<&StoredMetric>> = HashMap::new();

        for m in self.metrics() {
            if m.metric_name != query.name {
                continue;
            }
            if let Some(ref svc) = query.service {
                if &m.service_name != svc {
                    continue;
                }
            }
            if m.timestamp < since {
                continue;
            }
            groups
                .entry((m.metric_name.clone(), m.service_name.clone()))
                .or_default()
                .push(m);
        }

        let mut series: Vec<MetricSeries> = groups
            .into_iter()
            .map(|((metric_name, service_name), mut metrics)| {
                // Sort chronologically
                metrics.sort_by_key(|m| m.timestamp);

                let metric_type = metrics.first().map(|m| m.metric_type).unwrap_or(MetricType::Gauge);
                let unit = metrics.first().and_then(|m| m.unit.clone());

                let points: Vec<MetricSeriesPoint> = metrics
                    .iter()
                    .map(|m| MetricSeriesPoint {
                        t: m.timestamp.timestamp_millis(),
                        v: m.value,
                    })
                    .collect();

                MetricSeries {
                    metric_name,
                    service_name,
                    metric_type,
                    unit,
                    points,
                }
            })
            .collect();

        // Sort series by service_name for consistent ordering
        series.sort_by(|a, b| a.service_name.cmp(&b.service_name));

        MetricSeriesResponse { series }
    }

    /// Get related telemetry for a trace: logs and metrics from the same services
    /// within the trace's time window.
    pub fn get_related(&self, trace_id: &str) -> RelatedTelemetry {
        let detail = match self.get_trace(trace_id) {
            Some(d) => d,
            None => {
                return RelatedTelemetry {
                    logs: vec![],
                    metrics: vec![],
                }
            }
        };

        let services: std::collections::HashSet<&str> = detail
            .spans
            .iter()
            .map(|s| s.service_name.as_str())
            .collect();

        let min_time = detail
            .spans
            .iter()
            .map(|s| s.start_time)
            .min()
            .unwrap_or_else(Utc::now);
        let max_time = detail
            .spans
            .iter()
            .map(|s| s.end_time)
            .max()
            .unwrap_or_else(Utc::now);

        // Add some buffer around the time window
        let buffer = chrono::Duration::seconds(5);
        let window_start = min_time - buffer;
        let window_end = max_time + buffer;

        let logs: Vec<StoredLog> = self
            .logs()
            .iter()
            .filter(|l| {
                services.contains(l.service_name.as_str())
                    && l.timestamp >= window_start
                    && l.timestamp <= window_end
            })
            .cloned()
            .collect();

        let metrics: Vec<StoredMetric> = self
            .metrics()
            .iter()
            .filter(|m| {
                services.contains(m.service_name.as_str())
                    && m.timestamp >= window_start
                    && m.timestamp <= window_end
            })
            .cloned()
            .collect();

        RelatedTelemetry { logs, metrics }
    }
}

fn parse_severity(s: &str) -> LogSeverity {
    match s.to_lowercase().as_str() {
        "trace" => LogSeverity::Trace,
        "debug" => LogSeverity::Debug,
        "info" => LogSeverity::Info,
        "warn" | "warning" => LogSeverity::Warn,
        "error" => LogSeverity::Error,
        "fatal" => LogSeverity::Fatal,
        _ => LogSeverity::Trace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::otel::types::*;
    use chrono::Utc;
    use std::time::Duration;

    fn make_span(trace_id: &str, service: &str, op: &str, status: SpanStatus) -> StoredSpan {
        StoredSpan {
            record_id: 0,
            trace_id: trace_id.to_string(),
            span_id: format!("{}-{}", service, op),
            parent_span_id: None,
            service_name: service.to_string(),
            operation_name: op.to_string(),
            start_time: Utc::now(),
            end_time: Utc::now() + chrono::Duration::milliseconds(100),
            duration_ms: 100,
            status,
            status_message: None,
            attributes: vec![],
            kind: SpanKind::Server,
        }
    }

    fn make_log_with_trace(
        service: &str,
        severity: LogSeverity,
        trace_id: Option<&str>,
    ) -> StoredLog {
        StoredLog {
            record_id: 0,
            timestamp: Utc::now(),
            service_name: service.to_string(),
            severity,
            body: format!("log from {}", service),
            trace_id: trace_id.map(|s| s.to_string()),
            span_id: None,
            attributes: vec![],
        }
    }

    fn make_metric(service: &str, name: &str, value: f64) -> StoredMetric {
        StoredMetric {
            record_id: 0,
            timestamp: Utc::now(),
            service_name: service.to_string(),
            metric_name: name.to_string(),
            metric_type: MetricType::Gauge,
            value,
            attributes: vec![],
            unit: None,
        }
    }

    #[test]
    fn query_traces_by_service() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(3600));
        store.insert_span(make_span("t1", "api", "GET /users", SpanStatus::Ok));
        store.insert_span(make_span("t2", "web", "render", SpanStatus::Ok));
        store.insert_span(make_span("t3", "api", "POST /users", SpanStatus::Ok));

        let results = store.query_traces(&TraceQuery {
            service: Some("api".to_string()),
            ..Default::default()
        });
        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .all(|t| t.services.contains(&"api".to_string())));
    }

    #[test]
    fn query_traces_by_error_status() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(3600));
        store.insert_span(make_span("t1", "api", "op", SpanStatus::Ok));
        store.insert_span(make_span("t2", "api", "op", SpanStatus::Error));

        let results = store.query_traces(&TraceQuery {
            status: Some("error".to_string()),
            ..Default::default()
        });
        assert_eq!(results.len(), 1);
        assert!(results[0].has_error);
    }

    #[test]
    fn query_traces_by_min_duration() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(3600));
        let mut fast = make_span("t1", "api", "fast", SpanStatus::Ok);
        fast.end_time = fast.start_time + chrono::Duration::milliseconds(10);

        let mut slow = make_span("t2", "api", "slow", SpanStatus::Ok);
        slow.end_time = slow.start_time + chrono::Duration::milliseconds(500);

        store.insert_span(fast);
        store.insert_span(slow);

        let results = store.query_traces(&TraceQuery {
            min_duration_ms: Some(100),
            ..Default::default()
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].trace_id, "t2");
    }

    #[test]
    fn query_logs_by_severity() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(3600));
        store.insert_log(make_log_with_trace("api", LogSeverity::Debug, None));
        store.insert_log(make_log_with_trace("api", LogSeverity::Error, None));
        store.insert_log(make_log_with_trace("api", LogSeverity::Info, None));

        let results = store.query_logs(&LogQuery {
            severity: Some("error".to_string()),
            ..Default::default()
        });
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn query_logs_by_trace_id() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(3600));
        store.insert_log(make_log_with_trace(
            "api",
            LogSeverity::Info,
            Some("trace-abc"),
        ));
        store.insert_log(make_log_with_trace("api", LogSeverity::Info, None));

        let results = store.query_logs(&LogQuery {
            trace_id: Some("trace-abc".to_string()),
            ..Default::default()
        });
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn query_logs_by_search_text() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(3600));
        let mut log1 = make_log_with_trace("api", LogSeverity::Info, None);
        log1.body = "user created successfully".to_string();
        let mut log2 = make_log_with_trace("api", LogSeverity::Error, None);
        log2.body = "database connection failed".to_string();

        store.insert_log(log1);
        store.insert_log(log2);

        let results = store.query_logs(&LogQuery {
            search: Some("database".to_string()),
            ..Default::default()
        });
        assert_eq!(results.len(), 1);
        assert!(results[0].body.contains("database"));
    }

    #[test]
    fn query_metrics_by_name_substring() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(3600));
        store.insert_metric(make_metric("api", "http.duration", 42.0));
        store.insert_metric(make_metric("api", "http.count", 10.0));
        store.insert_metric(make_metric("api", "db.query_time", 5.0));

        // Substring match: "http" matches both http.duration and http.count
        let results = store.query_metrics(&MetricQuery {
            name: Some("http".to_string()),
            ..Default::default()
        });
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|m| m.metric_name.contains("http")));

        // Exact name still works as a substring match
        let results = store.query_metrics(&MetricQuery {
            name: Some("http.duration".to_string()),
            ..Default::default()
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].metric_name, "http.duration");
    }

    #[test]
    fn query_metrics_by_type() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(3600));
        store.insert_metric(make_metric("api", "http.duration", 42.0)); // Gauge by default
        let mut counter = make_metric("api", "http.count", 10.0);
        counter.metric_type = MetricType::Counter;
        store.insert_metric(counter);

        let results = store.query_metrics(&MetricQuery {
            metric_type: Some("Counter".to_string()),
            ..Default::default()
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].metric_name, "http.count");
    }

    #[test]
    fn query_traces_by_search() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(3600));
        store.insert_span(make_span("t1", "api", "GET /users", SpanStatus::Ok));
        store.insert_span(make_span("t2", "api", "POST /orders", SpanStatus::Ok));
        store.insert_span(make_span("t3", "web", "render home", SpanStatus::Ok));

        let results = store.query_traces(&TraceQuery {
            search: Some("users".to_string()),
            ..Default::default()
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].root_operation, "GET /users");

        // Case-insensitive
        let results = store.query_traces(&TraceQuery {
            search: Some("POST".to_string()),
            ..Default::default()
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].root_operation, "POST /orders");
    }

    #[test]
    fn get_related_for_trace() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(3600));
        store.insert_span(make_span("trace-x", "api", "op", SpanStatus::Ok));
        store.insert_log(make_log_with_trace(
            "api",
            LogSeverity::Info,
            Some("trace-x"),
        ));
        store.insert_metric(make_metric("api", "m1", 1.0));
        store.insert_log(make_log_with_trace("web", LogSeverity::Info, None));

        let related = store.get_related("trace-x");
        assert_eq!(related.logs.len(), 1);
        assert_eq!(related.metrics.len(), 1);
    }
}
