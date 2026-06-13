package otel

import (
	"fmt"
	"testing"
	"time"
)

func span(traceID, service, op string, durMs uint64, status SpanStatus) StoredSpan {
	now := time.Now()
	return StoredSpan{
		TraceID:       traceID,
		SpanID:        traceID + "-s",
		ServiceName:   service,
		OperationName: op,
		StartTime:     now.Add(-time.Duration(durMs) * time.Millisecond),
		EndTime:       now,
		DurationMs:    durMs,
		Status:        status,
		Kind:          SpanKindServer,
	}
}

func TestStoreInsertAndStatus(t *testing.T) {
	s := NewStore(100, 100, 100, time.Hour)
	s.InsertSpan(span("t1", "api", "GET /a", 10, SpanStatusOk))
	s.InsertSpan(span("t1", "api", "db.query", 5, SpanStatusOk))
	s.InsertSpan(span("t2", "web", "GET /b", 20, SpanStatusError))
	tid := "t1"
	s.InsertLog(StoredLog{Timestamp: time.Now(), ServiceName: "api", Severity: SeverityInfo, Body: "hello world", TraceID: &tid})
	s.InsertMetric(StoredMetric{Timestamp: time.Now(), ServiceName: "api", MetricName: "http_requests", MetricType: MetricCounter, Value: 42})

	st := s.Status()
	if st.TraceCount != 2 {
		t.Errorf("trace_count = %d, want 2", st.TraceCount)
	}
	if st.SpanCount != 3 || st.LogCount != 1 || st.MetricCount != 1 {
		t.Errorf("counts = %d/%d/%d", st.SpanCount, st.LogCount, st.MetricCount)
	}
	if len(st.Services) != 2 {
		t.Errorf("services = %v", st.Services)
	}
}

func TestQueryTracesFilters(t *testing.T) {
	s := NewStore(100, 100, 100, time.Hour)
	s.InsertSpan(span("t1", "api", "GET /users", 10, SpanStatusOk))
	s.InsertSpan(span("t2", "api", "GET /orders", 150, SpanStatusError))
	s.InsertSpan(span("t3", "web", "render", 30, SpanStatusOk))

	if got := s.QueryTraces(&SpanQuery{Service: "api"}); len(got) != 2 {
		t.Errorf("service filter: got %d traces", len(got))
	}
	if got := s.QueryTraces(&SpanQuery{OnlyError: true}); len(got) != 1 || got[0].TraceID != "t2" {
		t.Errorf("error filter: %+v", got)
	}
	if got := s.QueryTraces(&SpanQuery{Search: "orders"}); len(got) != 1 || got[0].TraceID != "t2" {
		t.Errorf("search filter: %+v", got)
	}
	if got := s.QueryTraces(&SpanQuery{MinDurationMs: 100}); len(got) != 1 || got[0].TraceID != "t2" {
		t.Errorf("min duration filter: %+v", got)
	}
	if got := s.QueryTraces(&SpanQuery{TraceID: "t3"}); len(got) != 1 || got[0].Services[0] != "web" {
		t.Errorf("trace_id filter: %+v", got)
	}
}

func TestQueryLogsFilters(t *testing.T) {
	s := NewStore(100, 100, 100, time.Hour)
	t1 := "t1"
	s.InsertLog(StoredLog{Timestamp: time.Now(), ServiceName: "api", Severity: SeverityError, Body: "connection refused", TraceID: &t1})
	s.InsertLog(StoredLog{Timestamp: time.Now(), ServiceName: "api", Severity: SeverityInfo, Body: "request ok"})
	s.InsertLog(StoredLog{Timestamp: time.Now(), ServiceName: "web", Severity: SeverityInfo, Body: "page served"})

	if got := s.QueryLogs(&LogQuery{Severity: "error"}); len(got) != 1 {
		t.Errorf("severity filter: %d", len(got))
	}
	if got := s.QueryLogs(&LogQuery{Search: "refused"}); len(got) != 1 {
		t.Errorf("search filter: %d", len(got))
	}
	if got := s.QueryLogs(&LogQuery{TraceID: "t1"}); len(got) != 1 {
		t.Errorf("trace_id filter: %d", len(got))
	}
	if got := s.QueryLogs(&LogQuery{Service: "web"}); len(got) != 1 {
		t.Errorf("service filter: %d", len(got))
	}
}

func TestSpanEviction(t *testing.T) {
	s := NewStore(5, 5, 5, time.Hour)
	for i := 0; i < 10; i++ {
		s.InsertSpan(span(fmt.Sprintf("t%d", i), "api", "op", 1, SpanStatusOk))
	}
	st := s.Status()
	if st.SpanCount != 5 {
		t.Errorf("span_count after eviction = %d, want 5", st.SpanCount)
	}
	if st.TraceCount != 5 {
		t.Errorf("trace_count after eviction = %d, want 5 (indexes must be cleaned)", st.TraceCount)
	}
	// Oldest traces must be gone, newest present.
	if got := s.QueryTraces(&SpanQuery{TraceID: "t0"}); len(got) != 0 {
		t.Errorf("evicted trace still queryable: %+v", got)
	}
	if got := s.QueryTraces(&SpanQuery{TraceID: "t9"}); len(got) != 1 {
		t.Errorf("newest trace missing")
	}
}

// Empty query results must be empty slices, not nil — the dashboard JSON API
// contract is `[]`, never `null` (the frontend calls .length on responses).
func TestQueriesNeverReturnNil(t *testing.T) {
	s := NewStore(10, 10, 10, time.Hour)
	if got := s.QueryTraces(&SpanQuery{Service: "ghost"}); got == nil {
		t.Error("QueryTraces returned nil")
	}
	if got := s.QueryLogs(&LogQuery{Service: "ghost"}); got == nil {
		t.Error("QueryLogs returned nil")
	}
	if got := s.QueryMetrics(&MetricQuery{MetricName: "ghost"}); got == nil {
		t.Error("QueryMetrics returned nil")
	}
	logs, metrics := s.GetRelated("no-such-trace")
	_ = logs
	_ = metrics
	// GetRelated on a missing trace returns empties via the early path; insert
	// a trace and query related to exercise the slice-building path.
	s.InsertSpan(span("t1", "api", "op", 1, SpanStatusOk))
	logs, metrics = s.GetRelated("t1")
	if logs == nil || metrics == nil {
		t.Error("GetRelated returned nil slices")
	}
}

// Eviction must stay correct at a larger max (the backing-array window walks
// and is reallocated as the ring fills past its initial position).
func TestEvictionAtLargeMax(t *testing.T) {
	max := 1500
	s := NewStore(max, max, max, time.Hour)
	total := max + 300 // overflow so eviction kicks in
	for i := 0; i < total; i++ {
		s.InsertSpan(span(fmt.Sprintf("t%d", i), "api", "op", 1, SpanStatusOk))
	}
	st := s.Status()
	if st.SpanCount != max {
		t.Errorf("span_count = %d, want capped at %d", st.SpanCount, max)
	}
	if st.TraceCount != max {
		t.Errorf("trace_count = %d, want %d (indexes must track eviction)", st.TraceCount, max)
	}
	// Oldest evicted, newest retained.
	if got := s.QueryTraces(&SpanQuery{TraceID: "t0"}); len(got) != 0 {
		t.Error("oldest trace t0 should have been evicted")
	}
	if got := s.QueryTraces(&SpanQuery{TraceID: fmt.Sprintf("t%d", total-1)}); len(got) != 1 {
		t.Error("newest trace should be retained")
	}
}

func TestGetTraceAndRelated(t *testing.T) {
	s := NewStore(100, 100, 100, time.Hour)
	s.InsertSpan(span("t1", "api", "GET /a", 10, SpanStatusOk))
	tid := "t1"
	s.InsertLog(StoredLog{Timestamp: time.Now(), ServiceName: "api", Severity: SeverityInfo, Body: "in trace", TraceID: &tid})

	detail, ok := s.GetTrace("t1")
	if !ok || len(detail.Spans) != 1 {
		t.Fatalf("GetTrace: ok=%v detail=%+v", ok, detail)
	}
	logs, _ := s.GetRelated("t1")
	if len(logs) != 1 || logs[0].Body != "in trace" {
		t.Errorf("GetRelated logs = %+v", logs)
	}
}
