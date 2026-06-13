package otel

import (
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Store is an in-memory ring-buffer telemetry store with secondary indexes.
type Store struct {
	mu      sync.RWMutex
	spans   []StoredSpan
	logs    []StoredLog
	metrics []StoredMetric
	nextID  uint64

	// Secondary indexes (record_id → indices)
	traceIndex   map[string][]uint64
	svcSpanIdx   map[string][]uint64
	errorSpans   map[uint64]bool
	svcLogIdx    map[string][]uint64
	svcMetricIdx map[string][]uint64

	maxSpans   int
	maxLogs    int
	maxMetrics int
	retention  time.Duration
}

// bufferCapLimit bounds the preallocated capacity of each telemetry ring
// buffer. Preallocating to the configured max reserves backing-array address
// space up front, but those zero pages are demand-paged and never become
// resident until telemetry actually fills them — so the up-front cost is
// virtual (VmData), not real RAM (VmRSS). Preallocating also keeps the ingest
// hot path realloc-free as the buffers fill.
const bufferCapLimit = 65536

// NewStore creates a store with given limits and retention.
func NewStore(maxSpans, maxLogs, maxMetrics int, retention time.Duration) *Store {
	return &Store{
		spans:        make([]StoredSpan, 0, min64(maxSpans, bufferCapLimit)),
		logs:         make([]StoredLog, 0, min64(maxLogs, bufferCapLimit)),
		metrics:      make([]StoredMetric, 0, min64(maxMetrics, bufferCapLimit)),
		nextID:       1,
		traceIndex:   make(map[string][]uint64),
		svcSpanIdx:   make(map[string][]uint64),
		errorSpans:   make(map[uint64]bool),
		svcLogIdx:    make(map[string][]uint64),
		svcMetricIdx: make(map[string][]uint64),
		maxSpans:     maxSpans,
		maxLogs:      maxLogs,
		maxMetrics:   maxMetrics,
		retention:    retention,
	}
}

func (s *Store) nextRecordID() uint64 {
	id := s.nextID
	s.nextID++
	return id
}

// --- Span operations ---

func (s *Store) InsertSpan(span StoredSpan) {
	s.mu.Lock()
	defer s.mu.Unlock()
	span.RecordID = s.nextRecordID()
	if len(s.spans) >= s.maxSpans {
		evicted := s.spans[0]
		s.spans = s.spans[1:]
		s.removeSpanIndexes(&evicted)
	}
	s.traceIndex[span.TraceID] = append(s.traceIndex[span.TraceID], span.RecordID)
	s.svcSpanIdx[span.ServiceName] = append(s.svcSpanIdx[span.ServiceName], span.RecordID)
	if span.Status == SpanStatusError {
		s.errorSpans[span.RecordID] = true
	}
	s.spans = append(s.spans, span)
}

func (s *Store) removeSpanIndexes(span *StoredSpan) {
	removeID(s.traceIndex, span.TraceID, span.RecordID)
	removeID(s.svcSpanIdx, span.ServiceName, span.RecordID)
	delete(s.errorSpans, span.RecordID)
}

// --- Log operations ---

func (s *Store) InsertLog(log StoredLog) {
	s.mu.Lock()
	defer s.mu.Unlock()
	log.RecordID = s.nextRecordID()
	if len(s.logs) >= s.maxLogs {
		evicted := s.logs[0]
		s.logs = s.logs[1:]
		removeID(s.svcLogIdx, evicted.ServiceName, evicted.RecordID)
	}
	s.svcLogIdx[log.ServiceName] = append(s.svcLogIdx[log.ServiceName], log.RecordID)
	s.logs = append(s.logs, log)
}

// --- Metric operations ---

func (s *Store) InsertMetric(m StoredMetric) {
	s.mu.Lock()
	defer s.mu.Unlock()
	m.RecordID = s.nextRecordID()
	if len(s.metrics) >= s.maxMetrics {
		evicted := s.metrics[0]
		s.metrics = s.metrics[1:]
		removeID(s.svcMetricIdx, evicted.ServiceName, evicted.RecordID)
	}
	s.svcMetricIdx[m.ServiceName] = append(s.svcMetricIdx[m.ServiceName], m.RecordID)
	s.metrics = append(s.metrics, m)
}

// --- Retention sweeper ---

// SweepRetention removes records older than the retention window.
func (s *Store) SweepRetention() {
	if s.retention == 0 {
		return
	}
	cutoff := time.Now().Add(-s.retention)
	s.mu.Lock()
	defer s.mu.Unlock()

	for len(s.spans) > 0 && s.spans[0].StartTime.Before(cutoff) {
		s.removeSpanIndexes(&s.spans[0])
		s.spans = s.spans[1:]
	}
	for len(s.logs) > 0 && s.logs[0].Timestamp.Before(cutoff) {
		removeID(s.svcLogIdx, s.logs[0].ServiceName, s.logs[0].RecordID)
		s.logs = s.logs[1:]
	}
	for len(s.metrics) > 0 && s.metrics[0].Timestamp.Before(cutoff) {
		removeID(s.svcMetricIdx, s.metrics[0].ServiceName, s.metrics[0].RecordID)
		s.metrics = s.metrics[1:]
	}
}

// --- Query methods ---

type SpanQuery struct {
	Service       string
	TraceID       string
	OnlyError     bool
	OnlyOk        bool
	Search        string
	MinDurationMs float64
	Limit         int
	Offset        int
}

type LogQuery struct {
	Service  string
	Severity string
	Search   string
	TraceID  string
	Limit    int
	Offset   int
	Since    *time.Time
}

type MetricQuery struct {
	Service    string
	MetricName string
	Limit      int
	Offset     int
}

type MetricSeriesQuery struct {
	Service    string
	MetricName string
}

func (s *Store) QueryTraces(q *SpanQuery) []TraceSummary {
	s.mu.RLock()
	defer s.mu.RUnlock()

	limit := q.Limit
	if limit <= 0 {
		limit = 100
	}

	// Collect relevant trace IDs, newest first.
	seen := make(map[string]bool)
	var traceIDs []string

	searchLower := strings.ToLower(q.Search)

	for i := len(s.spans) - 1; i >= 0; i-- {
		sp := &s.spans[i]
		if q.Service != "" && sp.ServiceName != q.Service {
			continue
		}
		if q.TraceID != "" && sp.TraceID != q.TraceID {
			continue
		}
		if q.OnlyError && !s.errorSpans[sp.RecordID] {
			continue
		}
		if searchLower != "" {
			opLower := strings.ToLower(sp.OperationName)
			if !strings.Contains(opLower, searchLower) && !strings.Contains(sp.TraceID, searchLower) {
				continue
			}
		}
		if !seen[sp.TraceID] {
			seen[sp.TraceID] = true
			traceIDs = append(traceIDs, sp.TraceID)
		}
		if len(traceIDs) >= limit+q.Offset {
			break
		}
	}

	if q.Offset >= len(traceIDs) {
		return []TraceSummary{}
	}
	traceIDs = traceIDs[q.Offset:]
	if len(traceIDs) > limit {
		traceIDs = traceIDs[:limit]
	}

	summaries := make([]TraceSummary, 0, len(traceIDs))
	for _, tid := range traceIDs {
		sum, ok := s.buildTraceSummary(tid)
		if !ok {
			continue
		}
		if q.MinDurationMs > 0 && float64(sum.DurationMs) < q.MinDurationMs {
			continue
		}
		if q.OnlyOk && sum.HasError {
			continue
		}
		summaries = append(summaries, sum)
	}
	return summaries
}

func (s *Store) buildTraceSummary(traceID string) (TraceSummary, bool) {
	ids := s.traceIndex[traceID]
	if len(ids) == 0 {
		return TraceSummary{}, false
	}

	sum := TraceSummary{TraceID: traceID}
	svcSet := make(map[string]bool)
	hasError := false
	var earliest time.Time
	var latest time.Time
	var rootOp string

	for _, rid := range ids {
		sp := s.spanByID(rid)
		if sp == nil {
			continue
		}
		svcSet[sp.ServiceName] = true
		if sp.Status == SpanStatusError {
			hasError = true
		}
		sum.SpanCount++
		if earliest.IsZero() || sp.StartTime.Before(earliest) {
			earliest = sp.StartTime
		}
		end := sp.EndTime
		if end.After(latest) {
			latest = end
		}
		if sp.ParentSpanID == nil {
			rootOp = sp.OperationName
			// Extract HTTP status from the root span. Rust parity: only the
			// stable semconv key — legacy http.status_code is intentionally
			// NOT read (the dashboard badge falls back to Ok/Error).
			for _, attr := range sp.Attributes {
				if attr[0] == "http.response.status_code" {
					if code, err := strconv.Atoi(attr[1]); err == nil {
						sum.HTTPStatus = &code
					}
				}
			}
		}
	}

	if sum.SpanCount == 0 {
		return TraceSummary{}, false
	}

	for svc := range svcSet {
		sum.Services = append(sum.Services, svc)
	}
	sort.Strings(sum.Services)
	sum.RootOperation = rootOp
	sum.HasError = hasError
	sum.StartTime = earliest
	if !latest.IsZero() && !earliest.IsZero() {
		sum.DurationMs = uint64(latest.Sub(earliest).Milliseconds())
	}
	return sum, true
}

func (s *Store) GetTrace(traceID string) (TraceDetail, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	ids := s.traceIndex[traceID]
	if len(ids) == 0 {
		return TraceDetail{}, false
	}
	spans := make([]StoredSpan, 0, len(ids))
	for _, rid := range ids {
		if sp := s.spanByID(rid); sp != nil {
			spans = append(spans, *sp)
		}
	}
	return TraceDetail{TraceID: traceID, Spans: spans}, true
}

func (s *Store) GetRelated(traceID string) ([]StoredLog, []StoredMetric) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	// Collect services in this trace.
	svcs := make(map[string]bool)
	for _, sp := range s.spans {
		if sp.TraceID == traceID {
			svcs[sp.ServiceName] = true
		}
	}

	// Find trace start/end window.
	var earliest, latest time.Time
	for _, sp := range s.spans {
		if sp.TraceID != traceID {
			continue
		}
		if earliest.IsZero() || sp.StartTime.Before(earliest) {
			earliest = sp.StartTime
		}
		if sp.EndTime.After(latest) {
			latest = sp.EndTime
		}
	}
	window := 2 * time.Minute
	start := earliest.Add(-window)
	end := latest.Add(window)

	logs := []StoredLog{}
	for _, l := range s.logs {
		if !svcs[l.ServiceName] {
			continue
		}
		if l.Timestamp.Before(start) || l.Timestamp.After(end) {
			continue
		}
		logs = append(logs, l)
	}

	metrics := []StoredMetric{}
	for _, m := range s.metrics {
		if !svcs[m.ServiceName] {
			continue
		}
		if m.Timestamp.Before(start) || m.Timestamp.After(end) {
			continue
		}
		metrics = append(metrics, m)
	}

	return logs, metrics
}

func (s *Store) QueryLogs(q *LogQuery) []StoredLog {
	s.mu.RLock()
	defer s.mu.RUnlock()

	limit := q.Limit
	if limit <= 0 {
		limit = 100
	}

	searchLower := strings.ToLower(q.Search)

	result := []StoredLog{}
	for i := len(s.logs) - 1; i >= 0 && len(result) < limit+q.Offset; i-- {
		l := &s.logs[i]
		if q.Service != "" && l.ServiceName != q.Service {
			continue
		}
		if q.Severity != "" {
			// Minimum-level filter (Rust parity): severity=Warn shows Warn+Error+Fatal.
			minRank := SeverityFromString(strings.ToLower(q.Severity)).Rank()
			if l.Severity.Rank() < minRank {
				continue
			}
		}
		if q.TraceID != "" && (l.TraceID == nil || *l.TraceID != q.TraceID) {
			continue
		}
		if q.Since != nil && l.Timestamp.Before(*q.Since) {
			break
		}
		if searchLower != "" && !strings.Contains(strings.ToLower(l.Body), searchLower) {
			continue
		}
		result = append(result, *l)
	}
	if q.Offset >= len(result) {
		return []StoredLog{}
	}
	result = result[q.Offset:]
	if len(result) > limit {
		result = result[:limit]
	}
	return result
}

func (s *Store) QueryMetrics(q *MetricQuery) []StoredMetric {
	s.mu.RLock()
	defer s.mu.RUnlock()

	limit := q.Limit
	if limit <= 0 {
		limit = 100
	}

	result := []StoredMetric{}
	for i := len(s.metrics) - 1; i >= 0 && len(result) < limit+q.Offset; i-- {
		m := &s.metrics[i]
		if q.Service != "" && m.ServiceName != q.Service {
			continue
		}
		if q.MetricName != "" && m.MetricName != q.MetricName {
			continue
		}
		result = append(result, *m)
	}
	if q.Offset >= len(result) {
		return []StoredMetric{}
	}
	result = result[q.Offset:]
	if len(result) > limit {
		result = result[:limit]
	}
	return result
}

// MetricPoint is a time-series datapoint.
type MetricPoint struct {
	Timestamp time.Time `json:"timestamp"`
	Value     float64   `json:"value"`
}

func (s *Store) GetMetricSeries(q *MetricSeriesQuery) []MetricPoint {
	s.mu.RLock()
	defer s.mu.RUnlock()

	pts := []MetricPoint{}
	for _, m := range s.metrics {
		if q.MetricName != "" && m.MetricName != q.MetricName {
			continue
		}
		if q.Service != "" && m.ServiceName != q.Service {
			continue
		}
		pts = append(pts, MetricPoint{Timestamp: m.Timestamp, Value: m.Value})
	}
	return pts
}

func (s *Store) Status() SystemStatus {
	s.mu.RLock()
	defer s.mu.RUnlock()

	svcSet := make(map[string]bool)
	for _, sp := range s.spans {
		svcSet[sp.ServiceName] = true
	}
	for _, l := range s.logs {
		svcSet[l.ServiceName] = true
	}
	svcs := make([]string, 0, len(svcSet))
	for svc := range svcSet {
		svcs = append(svcs, svc)
	}
	sort.Strings(svcs)

	return SystemStatus{
		TraceCount:  len(s.traceIndex),
		SpanCount:   len(s.spans),
		LogCount:    len(s.logs),
		MetricCount: len(s.metrics),
		Services:    svcs,
	}
}

func (s *Store) spanByID(recordID uint64) *StoredSpan {
	for i := range s.spans {
		if s.spans[i].RecordID == recordID {
			return &s.spans[i]
		}
	}
	return nil
}

func removeID(m map[string][]uint64, key string, id uint64) {
	ids := m[key]
	newIDs := ids[:0]
	for _, v := range ids {
		if v != id {
			newIDs = append(newIDs, v)
		}
	}
	if len(newIDs) == 0 {
		delete(m, key)
	} else {
		m[key] = newIDs
	}
}

func min64(a, b int) int {
	if a < b {
		return a
	}
	return b
}
