// Package otel provides in-process OTLP telemetry storage and a receiver.
package otel

import (
	"encoding/json"
	"time"
)

// --- Span types ---

type SpanStatus string

const (
	SpanStatusOk    SpanStatus = "Ok"
	SpanStatusError SpanStatus = "Error"
	SpanStatusUnset SpanStatus = "Unset"
)

type SpanKind string

const (
	SpanKindInternal SpanKind = "Internal"
	SpanKindServer   SpanKind = "Server"
	SpanKindClient   SpanKind = "Client"
	SpanKindProducer SpanKind = "Producer"
	SpanKindConsumer SpanKind = "Consumer"
)

type SpanEvent struct {
	Name       string        `json:"name"`
	Timestamp  time.Time     `json:"timestamp"`
	Attributes [][2]string   `json:"attributes"`
}

type StoredSpan struct {
	RecordID      uint64     `json:"record_id"`
	TraceID       string     `json:"trace_id"`
	SpanID        string     `json:"span_id"`
	ParentSpanID  *string    `json:"parent_span_id"`
	ServiceName   string     `json:"service_name"`
	OperationName string     `json:"operation_name"`
	StartTime     time.Time  `json:"start_time"`
	EndTime       time.Time  `json:"end_time"`
	DurationMs    uint64     `json:"duration_ms"`
	Status        SpanStatus `json:"status"`
	StatusMessage *string    `json:"status_message"`
	Attributes    [][2]string `json:"attributes"`
	Kind          SpanKind   `json:"kind"`
	Events        []SpanEvent `json:"events"`
}

// --- Log types ---

type LogSeverity string

const (
	SeverityTrace LogSeverity = "Trace"
	SeverityDebug LogSeverity = "Debug"
	SeverityInfo  LogSeverity = "Info"
	SeverityWarn  LogSeverity = "Warn"
	SeverityError LogSeverity = "Error"
	SeverityFatal LogSeverity = "Fatal"
)

func SeverityFromNumber(n int32) LogSeverity {
	switch {
	case n >= 1 && n <= 4:
		return SeverityTrace
	case n >= 5 && n <= 8:
		return SeverityDebug
	case n >= 9 && n <= 12:
		return SeverityInfo
	case n >= 13 && n <= 16:
		return SeverityWarn
	case n >= 17 && n <= 20:
		return SeverityError
	case n >= 21 && n <= 24:
		return SeverityFatal
	default:
		return SeverityInfo
	}
}

// Rank orders severities for minimum-level filtering (Trace < … < Fatal).
func (s LogSeverity) Rank() int {
	switch s {
	case SeverityTrace:
		return 0
	case SeverityDebug:
		return 1
	case SeverityInfo:
		return 2
	case SeverityWarn:
		return 3
	case SeverityError:
		return 4
	case SeverityFatal:
		return 5
	default:
		return 2
	}
}

func SeverityFromString(s string) LogSeverity {
	switch s {
	case "trace":
		return SeverityTrace
	case "debug":
		return SeverityDebug
	case "warn", "warning":
		return SeverityWarn
	case "error":
		return SeverityError
	case "fatal":
		return SeverityFatal
	default:
		return SeverityInfo
	}
}

type StoredLog struct {
	RecordID    uint64      `json:"record_id"`
	Timestamp   time.Time   `json:"timestamp"`
	ServiceName string      `json:"service_name"`
	Severity    LogSeverity `json:"severity"`
	Body        string      `json:"body"`
	TraceID     *string     `json:"trace_id"`
	SpanID      *string     `json:"span_id"`
	Attributes  [][2]string `json:"attributes"`
}

// --- Metric types ---

type MetricType string

const (
	MetricGauge     MetricType = "Gauge"
	MetricCounter   MetricType = "Counter"
	MetricHistogram MetricType = "Histogram"
)

type StoredMetric struct {
	RecordID    uint64      `json:"record_id"`
	Timestamp   time.Time   `json:"timestamp"`
	ServiceName string      `json:"service_name"`
	MetricName  string      `json:"metric_name"`
	MetricType  MetricType  `json:"metric_type"`
	Value       float64     `json:"value"`
	Attributes  [][2]string `json:"attributes"`
	Unit        *string     `json:"unit"`
}

// --- WS event types (must match Rust serde(tag = "type", content = "payload")) ---

type WSEvent struct {
	Type    string          `json:"type"`
	Payload json.RawMessage `json:"payload"`
}

type TraceUpdatePayload struct {
	TraceID    string `json:"trace_id"`
	Service    string `json:"service"`
	DurationMs uint64 `json:"duration_ms"`
	HasError   bool   `json:"has_error"`
}

type LogRecordPayload struct {
	TraceID  *string `json:"trace_id"`
	Severity string  `json:"severity"`
	Body     string  `json:"body"`
	Service  string  `json:"service"`
}

type MetricUpdatePayload struct {
	Name    string  `json:"name"`
	Value   float64 `json:"value"`
	Service string  `json:"service"`
}

type ServiceStatusChangePayload struct {
	Service string `json:"service"`
	Status  string `json:"status"`
}

func MakeTraceUpdateEvent(p TraceUpdatePayload) WSEvent {
	b, _ := json.Marshal(p)
	return WSEvent{Type: "TraceUpdate", Payload: b}
}

func MakeLogRecordEvent(p LogRecordPayload) WSEvent {
	b, _ := json.Marshal(p)
	return WSEvent{Type: "LogRecord", Payload: b}
}

func MakeMetricUpdateEvent(p MetricUpdatePayload) WSEvent {
	b, _ := json.Marshal(p)
	return WSEvent{Type: "MetricUpdate", Payload: b}
}

func MakeServiceStatusEvent(p ServiceStatusChangePayload) WSEvent {
	b, _ := json.Marshal(p)
	return WSEvent{Type: "ServiceStatusChange", Payload: b}
}

// TraceSummary is the list-view for a single trace.
type TraceSummary struct {
	TraceID       string    `json:"trace_id"`
	Services      []string  `json:"services"`
	RootOperation string    `json:"root_operation"`
	DurationMs    uint64    `json:"duration_ms"`
	SpanCount     int       `json:"span_count"`
	HasError      bool      `json:"has_error"`
	StartTime     time.Time `json:"start_time"`
	HTTPStatus    *int      `json:"http_status,omitempty"`
}

// TraceDetail is the full expanded trace.
type TraceDetail struct {
	TraceID string       `json:"trace_id"`
	Spans   []StoredSpan `json:"spans"`
}

// SystemStatus summarises the store counts.
type SystemStatus struct {
	TraceCount  int      `json:"trace_count"`
	SpanCount   int      `json:"span_count"`
	LogCount    int      `json:"log_count"`
	MetricCount int      `json:"metric_count"`
	Services    []string `json:"services"`
}
