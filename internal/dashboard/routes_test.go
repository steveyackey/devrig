package dashboard

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/steveyackey/devrig/internal/otel"
	"github.com/steveyackey/devrig/internal/state"
)

func testServer(t *testing.T) (*Server, *otel.Store, string) {
	t.Helper()
	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "devrig.toml")
	if err := os.WriteFile(cfgPath, []byte("[project]\nname = \"test\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	store := otel.NewStore(1000, 1000, 1000, time.Hour)
	events := make(chan otel.WSEvent, 16)
	s := NewServer(ServerConfig{
		Port:       0,
		ConfigPath: cfgPath,
		StateDir:   dir,
		Store:      store,
		Events:     events,
	})
	return s, store, cfgPath
}

func get(t *testing.T, s *Server, path string, dst any) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequest(http.MethodGet, path, nil)
	rec := httptest.NewRecorder()
	s.mux.ServeHTTP(rec, req)
	if dst != nil && rec.Code == http.StatusOK {
		if err := json.Unmarshal(rec.Body.Bytes(), dst); err != nil {
			t.Fatalf("GET %s: bad JSON: %v\n%s", path, err, rec.Body.String())
		}
	}
	return rec
}

func TestStatusEndpoint(t *testing.T) {
	s, store, _ := testServer(t)
	store.InsertSpan(otel.StoredSpan{
		TraceID: "t1", SpanID: "s1", ServiceName: "api", OperationName: "GET /",
		StartTime: time.Now(), EndTime: time.Now(), Status: otel.SpanStatusOk, Kind: otel.SpanKindServer,
	})

	var status struct {
		TraceCount int      `json:"trace_count"`
		SpanCount  int      `json:"span_count"`
		Services   []string `json:"services"`
	}
	rec := get(t, s, "/api/status", &status)
	if rec.Code != http.StatusOK {
		t.Fatalf("status code %d", rec.Code)
	}
	if status.TraceCount != 1 || status.SpanCount != 1 {
		t.Errorf("status = %+v", status)
	}
}

func TestTracesEndpointFilters(t *testing.T) {
	s, store, _ := testServer(t)
	now := time.Now()
	store.InsertSpan(otel.StoredSpan{TraceID: "t1", SpanID: "a", ServiceName: "api", OperationName: "GET /users", StartTime: now.Add(-10 * time.Millisecond), EndTime: now, DurationMs: 10, Status: otel.SpanStatusOk, Kind: otel.SpanKindServer})
	store.InsertSpan(otel.StoredSpan{TraceID: "t2", SpanID: "b", ServiceName: "api", OperationName: "GET /orders", StartTime: now.Add(-200 * time.Millisecond), EndTime: now, DurationMs: 200, Status: otel.SpanStatusError, Kind: otel.SpanKindServer})

	var traces []map[string]any
	get(t, s, "/api/traces?status=error", &traces)
	if len(traces) != 1 || traces[0]["trace_id"] != "t2" {
		t.Errorf("status=error: %+v", traces)
	}

	traces = nil
	get(t, s, "/api/traces?search=users", &traces)
	if len(traces) != 1 || traces[0]["trace_id"] != "t1" {
		t.Errorf("search=users: %+v", traces)
	}

	traces = nil
	get(t, s, "/api/traces?min_duration_ms=100", &traces)
	if len(traces) != 1 || traces[0]["trace_id"] != "t2" {
		t.Errorf("min_duration_ms: %+v", traces)
	}
}

func TestTraceDetailAndRelated(t *testing.T) {
	s, store, _ := testServer(t)
	now := time.Now()
	store.InsertSpan(otel.StoredSpan{TraceID: "t1", SpanID: "a", ServiceName: "api", OperationName: "GET /", StartTime: now, EndTime: now, Status: otel.SpanStatusOk, Kind: otel.SpanKindServer})
	tid := "t1"
	store.InsertLog(otel.StoredLog{Timestamp: now, ServiceName: "api", Severity: otel.SeverityInfo, Body: "log line", TraceID: &tid})

	var detail struct {
		TraceID string           `json:"trace_id"`
		Spans   []map[string]any `json:"spans"`
	}
	get(t, s, "/api/traces/t1", &detail)
	if detail.TraceID != "t1" || len(detail.Spans) != 1 {
		t.Errorf("detail = %+v", detail)
	}

	var related struct {
		Logs    []map[string]any `json:"logs"`
		Metrics []map[string]any `json:"metrics"`
	}
	get(t, s, "/api/traces/t1/related", &related)
	if len(related.Logs) != 1 {
		t.Errorf("related = %+v", related)
	}

	rec := get(t, s, "/api/traces/nonexistent", nil)
	if rec.Code != http.StatusNotFound {
		t.Errorf("missing trace: code %d", rec.Code)
	}
}

func TestServicesEndpoint(t *testing.T) {
	s, _, cfgPath := testServer(t)
	// Config with a link so the config-derived entries are exercised too.
	if err := os.WriteFile(cfgPath, []byte("[project]\nname = \"test\"\n\n[links]\ngrafana = \"http://localhost:3001/dash\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	port := uint16(3000)
	phase := "running"
	code := 0
	st := &state.ProjectState{
		Slug: "test",
		Services: map[string]state.ServiceState{
			"api": {PID: 42, Port: &port, Phase: &phase, ExitCode: &code, PortAuto: true},
		},
		Docker: map[string]state.DockerState{
			"postgres": {ContainerName: "devrig-test-postgres", Port: &port},
		},
	}
	if err := st.Save(s.cfg.StateDir); err != nil {
		t.Fatal(err)
	}

	var svcs []map[string]any
	get(t, s, "/api/services", &svcs)
	byName := map[string]map[string]any{}
	for _, e := range svcs {
		byName[e["name"].(string)] = e
	}
	if len(svcs) != 3 {
		t.Fatalf("expected service+docker+link, got %d: %+v", len(svcs), svcs)
	}
	api := byName["api"]
	if api["kind"] != "service" || api["phase"] != "running" || api["port"] != float64(3000) || api["port_auto"] != true {
		t.Errorf("service entry = %+v", api)
	}
	pg := byName["postgres"]
	if pg["kind"] != "docker" || pg["phase"] != "running" {
		t.Errorf("docker entry = %+v", pg)
	}
	gr := byName["grafana"]
	if gr["kind"] != "link" || gr["url"] != "http://localhost:3001/dash" || gr["port"] != float64(3001) {
		t.Errorf("link entry = %+v", gr)
	}
}

func TestConfigGetAndPut(t *testing.T) {
	s, _, cfgPath := testServer(t)

	var cfg struct {
		Content string `json:"content"`
		Hash    string `json:"hash"`
	}
	get(t, s, "/api/config", &cfg)
	if !strings.Contains(cfg.Content, "[project]") || cfg.Hash == "" {
		t.Fatalf("config GET = %+v", cfg)
	}

	// PUT with correct hash succeeds.
	body, _ := json.Marshal(map[string]string{
		"content": "[project]\nname = \"renamed\"\n",
		"hash":    cfg.Hash,
	})
	req := httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(string(body)))
	rec := httptest.NewRecorder()
	s.mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("PUT code %d: %s", rec.Code, rec.Body.String())
	}
	data, _ := os.ReadFile(cfgPath)
	if !strings.Contains(string(data), "renamed") {
		t.Errorf("config not written: %s", data)
	}

	// PUT with stale hash conflicts.
	body, _ = json.Marshal(map[string]string{"content": "x", "hash": "deadbeef"})
	req = httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(string(body)))
	rec = httptest.NewRecorder()
	s.mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusConflict {
		t.Errorf("stale PUT code %d, want 409", rec.Code)
	}
}

func TestLogsAndMetricsEndpoints(t *testing.T) {
	s, store, _ := testServer(t)
	now := time.Now()
	store.InsertLog(otel.StoredLog{Timestamp: now, ServiceName: "api", Severity: otel.SeverityWarn, Body: "warned"})
	store.InsertMetric(otel.StoredMetric{Timestamp: now, ServiceName: "api", MetricName: "reqs", MetricType: otel.MetricCounter, Value: 7})

	var logs []map[string]any
	get(t, s, "/api/logs?severity=warn", &logs)
	if len(logs) != 1 || logs[0]["body"] != "warned" {
		t.Errorf("logs = %+v", logs)
	}

	var metrics []map[string]any
	get(t, s, "/api/metrics?metric_name=reqs", &metrics)
	if len(metrics) != 1 || metrics[0]["value"] != float64(7) {
		t.Errorf("metrics = %+v", metrics)
	}
}
