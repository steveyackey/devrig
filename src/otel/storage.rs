use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use chrono::Utc;

use super::types::{SpanStatus, StoredLog, StoredMetric, StoredSpan};

/// In-memory ring buffer storage for telemetry data with secondary indexes.
pub struct TelemetryStore {
    // Primary storage (ring buffers)
    spans: VecDeque<StoredSpan>,
    logs: VecDeque<StoredLog>,
    metrics: VecDeque<StoredMetric>,
    next_id: u64,

    // Secondary indexes for spans
    trace_index: HashMap<String, Vec<u64>>,
    service_span_index: HashMap<String, Vec<u64>>,
    error_spans: HashSet<u64>,

    // Secondary indexes for logs
    service_log_index: HashMap<String, Vec<u64>>,

    // Secondary indexes for metrics
    service_metric_index: HashMap<String, Vec<u64>>,

    // Configuration
    max_spans: usize,
    max_logs: usize,
    max_metrics: usize,
    retention: Duration,
}

impl TelemetryStore {
    pub fn new(max_spans: usize, max_logs: usize, max_metrics: usize, retention: Duration) -> Self {
        Self {
            spans: VecDeque::with_capacity(max_spans.min(65536)),
            logs: VecDeque::with_capacity(max_logs.min(65536)),
            metrics: VecDeque::with_capacity(max_metrics.min(65536)),
            next_id: 1,
            trace_index: HashMap::new(),
            service_span_index: HashMap::new(),
            error_spans: HashSet::new(),
            service_log_index: HashMap::new(),
            service_metric_index: HashMap::new(),
            max_spans,
            max_logs,
            max_metrics,
            retention,
        }
    }

    fn next_record_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // -----------------------------------------------------------------------
    // Span operations
    // -----------------------------------------------------------------------

    pub fn insert_span(&mut self, mut span: StoredSpan) {
        let record_id = self.next_record_id();
        span.record_id = record_id;

        // Evict if at capacity
        if self.spans.len() >= self.max_spans {
            if let Some(evicted) = self.spans.pop_front() {
                self.remove_span_from_indexes(&evicted);
            }
        }

        // Update indexes
        self.trace_index
            .entry(span.trace_id.clone())
            .or_default()
            .push(record_id);
        self.service_span_index
            .entry(span.service_name.clone())
            .or_default()
            .push(record_id);
        if span.status == SpanStatus::Error {
            self.error_spans.insert(record_id);
        }

        self.spans.push_back(span);
    }

    fn remove_span_from_indexes(&mut self, span: &StoredSpan) {
        if let Some(ids) = self.trace_index.get_mut(&span.trace_id) {
            ids.retain(|&id| id != span.record_id);
            if ids.is_empty() {
                self.trace_index.remove(&span.trace_id);
            }
        }
        if let Some(ids) = self.service_span_index.get_mut(&span.service_name) {
            ids.retain(|&id| id != span.record_id);
            if ids.is_empty() {
                self.service_span_index.remove(&span.service_name);
            }
        }
        self.error_spans.remove(&span.record_id);
    }

    // -----------------------------------------------------------------------
    // Log operations
    // -----------------------------------------------------------------------

    pub fn insert_log(&mut self, mut log: StoredLog) {
        let record_id = self.next_record_id();
        log.record_id = record_id;

        if self.logs.len() >= self.max_logs {
            if let Some(evicted) = self.logs.pop_front() {
                self.remove_log_from_indexes(&evicted);
            }
        }

        self.service_log_index
            .entry(log.service_name.clone())
            .or_default()
            .push(record_id);

        self.logs.push_back(log);
    }

    fn remove_log_from_indexes(&mut self, log: &StoredLog) {
        if let Some(ids) = self.service_log_index.get_mut(&log.service_name) {
            ids.retain(|&id| id != log.record_id);
            if ids.is_empty() {
                self.service_log_index.remove(&log.service_name);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Metric operations
    // -----------------------------------------------------------------------

    pub fn insert_metric(&mut self, mut metric: StoredMetric) {
        let record_id = self.next_record_id();
        metric.record_id = record_id;

        if self.metrics.len() >= self.max_metrics {
            if let Some(evicted) = self.metrics.pop_front() {
                self.remove_metric_from_indexes(&evicted);
            }
        }

        self.service_metric_index
            .entry(metric.service_name.clone())
            .or_default()
            .push(record_id);

        self.metrics.push_back(metric);
    }

    fn remove_metric_from_indexes(&mut self, metric: &StoredMetric) {
        if let Some(ids) = self.service_metric_index.get_mut(&metric.service_name) {
            ids.retain(|&id| id != metric.record_id);
            if ids.is_empty() {
                self.service_metric_index.remove(&metric.service_name);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Sweep expired entries
    // -----------------------------------------------------------------------

    pub fn sweep_expired(&mut self) {
        let cutoff = Utc::now() - chrono::Duration::from_std(self.retention).unwrap_or_default();

        while let Some(front) = self.spans.front() {
            if front.start_time < cutoff {
                let evicted = self.spans.pop_front().unwrap();
                self.remove_span_from_indexes(&evicted);
            } else {
                break;
            }
        }

        while let Some(front) = self.logs.front() {
            if front.timestamp < cutoff {
                let evicted = self.logs.pop_front().unwrap();
                self.remove_log_from_indexes(&evicted);
            } else {
                break;
            }
        }

        while let Some(front) = self.metrics.front() {
            if front.timestamp < cutoff {
                let evicted = self.metrics.pop_front().unwrap();
                self.remove_metric_from_indexes(&evicted);
            } else {
                break;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    pub fn get_span_count(&self) -> usize {
        self.spans.len()
    }

    pub fn get_log_count(&self) -> usize {
        self.logs.len()
    }

    pub fn get_metric_count(&self) -> usize {
        self.metrics.len()
    }

    pub fn spans(&self) -> &VecDeque<StoredSpan> {
        &self.spans
    }

    pub fn logs(&self) -> &VecDeque<StoredLog> {
        &self.logs
    }

    pub fn metrics(&self) -> &VecDeque<StoredMetric> {
        &self.metrics
    }

    pub fn trace_index(&self) -> &HashMap<String, Vec<u64>> {
        &self.trace_index
    }

    pub fn error_spans(&self) -> &HashSet<u64> {
        &self.error_spans
    }

    pub fn service_names(&self) -> Vec<String> {
        let mut names: HashSet<String> = HashSet::new();
        for key in self.service_span_index.keys() {
            names.insert(key.clone());
        }
        for key in self.service_log_index.keys() {
            names.insert(key.clone());
        }
        for key in self.service_metric_index.keys() {
            names.insert(key.clone());
        }
        let mut sorted: Vec<String> = names.into_iter().collect();
        sorted.sort();
        sorted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::otel::types::*;
    use chrono::Utc;

    fn make_span(trace_id: &str, service: &str, operation: &str, status: SpanStatus) -> StoredSpan {
        StoredSpan {
            record_id: 0,
            trace_id: trace_id.to_string(),
            span_id: format!("{}-{}", service, operation),
            parent_span_id: None,
            service_name: service.to_string(),
            operation_name: operation.to_string(),
            start_time: Utc::now(),
            end_time: Utc::now(),
            duration_ms: 100,
            status,
            status_message: None,
            attributes: vec![],
            kind: SpanKind::Server,
        }
    }

    fn make_log(service: &str, severity: LogSeverity) -> StoredLog {
        StoredLog {
            record_id: 0,
            timestamp: Utc::now(),
            service_name: service.to_string(),
            severity,
            body: format!("{:?} log from {}", severity, service),
            trace_id: None,
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
    fn insert_spans_up_to_capacity() {
        let mut store = TelemetryStore::new(5, 5, 5, Duration::from_secs(3600));
        for i in 0..5 {
            store.insert_span(make_span(
                &format!("trace-{}", i),
                "svc",
                "op",
                SpanStatus::Ok,
            ));
        }
        assert_eq!(store.get_span_count(), 5);
    }

    #[test]
    fn insert_past_capacity_evicts_oldest() {
        let mut store = TelemetryStore::new(3, 3, 3, Duration::from_secs(3600));
        for i in 0..4 {
            store.insert_span(make_span(
                &format!("trace-{}", i),
                "svc",
                "op",
                SpanStatus::Ok,
            ));
        }
        assert_eq!(store.get_span_count(), 3);
        // Oldest (trace-0) should be evicted
        assert!(!store.trace_index.contains_key("trace-0"));
        assert!(store.trace_index.contains_key("trace-1"));
        assert!(store.trace_index.contains_key("trace-3"));
    }

    #[test]
    fn trace_index_groups_spans() {
        let mut store = TelemetryStore::new(10, 10, 10, Duration::from_secs(3600));
        store.insert_span(make_span("trace-a", "svc1", "op1", SpanStatus::Ok));
        store.insert_span(make_span("trace-a", "svc2", "op2", SpanStatus::Ok));
        store.insert_span(make_span("trace-b", "svc1", "op3", SpanStatus::Ok));

        assert_eq!(store.trace_index["trace-a"].len(), 2);
        assert_eq!(store.trace_index["trace-b"].len(), 1);
    }

    #[test]
    fn error_spans_index() {
        let mut store = TelemetryStore::new(10, 10, 10, Duration::from_secs(3600));
        store.insert_span(make_span("t1", "svc", "op", SpanStatus::Ok));
        store.insert_span(make_span("t2", "svc", "op", SpanStatus::Error));

        assert_eq!(store.error_spans.len(), 1);
    }

    #[test]
    fn service_index_spans() {
        let mut store = TelemetryStore::new(10, 10, 10, Duration::from_secs(3600));
        store.insert_span(make_span("t1", "api", "op", SpanStatus::Ok));
        store.insert_span(make_span("t2", "web", "op", SpanStatus::Ok));
        store.insert_span(make_span("t3", "api", "op2", SpanStatus::Ok));

        assert_eq!(store.service_span_index["api"].len(), 2);
        assert_eq!(store.service_span_index["web"].len(), 1);
    }

    #[test]
    fn sweep_expired_removes_old() {
        let mut store = TelemetryStore::new(100, 100, 100, Duration::from_secs(1));
        // Insert a span with old timestamp
        let mut old_span = make_span("old", "svc", "op", SpanStatus::Ok);
        old_span.start_time = Utc::now() - chrono::Duration::seconds(60);
        store.insert_span(old_span);

        // Insert a fresh span
        store.insert_span(make_span("fresh", "svc", "op", SpanStatus::Ok));

        store.sweep_expired();
        assert_eq!(store.get_span_count(), 1);
        assert!(!store.trace_index.contains_key("old"));
        assert!(store.trace_index.contains_key("fresh"));
    }

    #[test]
    fn log_insert_and_evict() {
        let mut store = TelemetryStore::new(10, 2, 10, Duration::from_secs(3600));
        store.insert_log(make_log("svc1", LogSeverity::Info));
        store.insert_log(make_log("svc2", LogSeverity::Error));
        store.insert_log(make_log("svc3", LogSeverity::Warn));

        assert_eq!(store.get_log_count(), 2);
        // svc1 should be evicted
        assert!(!store.service_log_index.contains_key("svc1"));
    }

    #[test]
    fn index_cleanup_complete_after_eviction() {
        let mut store = TelemetryStore::new(2, 2, 2, Duration::from_secs(3600));
        // Insert 3 spans: first will be evicted
        store.insert_span(make_span("evict-me", "svc-old", "op", SpanStatus::Error));
        store.insert_span(make_span("keep1", "svc1", "op", SpanStatus::Ok));
        store.insert_span(make_span("keep2", "svc2", "op", SpanStatus::Ok));

        // Verify evicted span's indexes are cleaned
        assert!(!store.trace_index.contains_key("evict-me"));
        assert!(!store.service_span_index.contains_key("svc-old"));
        assert!(store.error_spans.is_empty()); // the error span was evicted

        assert_eq!(store.get_span_count(), 2);
    }
}
