package otel

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// otlpJSONTraces is a minimal OTLP/JSON export with spec-mandated hex IDs.
const otlpJSONTraces = `{
  "resourceSpans": [{
    "resource": {
      "attributes": [{"key": "service.name", "value": {"stringValue": "seeder"}}]
    },
    "scopeSpans": [{
      "spans": [{
        "traceId": "5b8efff798038103d269b633813fc60c",
        "spanId": "eee19b7ec3c1b174",
        "name": "GET /seeded",
        "kind": 2,
        "startTimeUnixNano": "1700000000000000000",
        "endTimeUnixNano": "1700000000150000000",
        "status": {"code": 1}
      }]
    }]
  }]
}`

func TestHTTPReceiverAcceptsOTLPJSON(t *testing.T) {
	store := NewStore(100, 100, 100, time.Hour)
	events := make(chan WSEvent, 16)
	r := NewReceiver(store, events)

	req := httptest.NewRequest(http.MethodPost, "/v1/traces", strings.NewReader(otlpJSONTraces))
	req.Header.Set("Content-Type", "application/json")
	rec := httptest.NewRecorder()
	r.handleHTTPTraces(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status %d: %s", rec.Code, rec.Body.String())
	}

	// Hex trace ID must round-trip exactly (protojson would base64-mangle it).
	traces := store.QueryTraces(&SpanQuery{})
	if len(traces) != 1 {
		t.Fatalf("traces = %+v", traces)
	}
	if traces[0].TraceID != "5b8efff798038103d269b633813fc60c" {
		t.Errorf("trace ID mangled: %q", traces[0].TraceID)
	}
	if traces[0].Services[0] != "seeder" {
		t.Errorf("service = %q", traces[0].Services[0])
	}
	if traces[0].DurationMs != 150 {
		t.Errorf("duration = %d, want 150", traces[0].DurationMs)
	}
}

func TestHTTPReceiverRejectsBadJSON(t *testing.T) {
	r := NewReceiver(NewStore(10, 10, 10, time.Hour), make(chan WSEvent, 1))
	req := httptest.NewRequest(http.MethodPost, "/v1/traces", strings.NewReader("{nope"))
	req.Header.Set("Content-Type", "application/json")
	rec := httptest.NewRecorder()
	r.handleHTTPTraces(rec, req)
	if rec.Code != http.StatusBadRequest {
		t.Errorf("status %d, want 400", rec.Code)
	}
}
