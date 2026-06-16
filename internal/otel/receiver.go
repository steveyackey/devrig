package otel

import (
	"context"
	"encoding/hex"
	"fmt"
	"net"
	"net/http"
	"time"

	"strings"

	"go.opentelemetry.io/collector/pdata/plog"
	"go.opentelemetry.io/collector/pdata/pmetric"
	"go.opentelemetry.io/collector/pdata/ptrace"
	"google.golang.org/grpc"
	"google.golang.org/protobuf/proto"

	collogspb "go.opentelemetry.io/proto/otlp/collector/logs/v1"
	colmetricspb "go.opentelemetry.io/proto/otlp/collector/metrics/v1"
	coltracepb "go.opentelemetry.io/proto/otlp/collector/trace/v1"
	commonpb "go.opentelemetry.io/proto/otlp/common/v1"
	logspb "go.opentelemetry.io/proto/otlp/logs/v1"
	metricspb "go.opentelemetry.io/proto/otlp/metrics/v1"
	tracepb "go.opentelemetry.io/proto/otlp/trace/v1"
)

// Receiver holds an in-process OTLP HTTP + gRPC listener.
type Receiver struct {
	store    *Store
	eventsCh chan<- WSEvent

	httpSrv *http.Server
	grpcSrv *grpc.Server
}

// NewReceiver creates an OTLP receiver that ingests into store and publishes
// WSEvents to eventsCh.
func NewReceiver(store *Store, events chan<- WSEvent) *Receiver {
	return &Receiver{store: store, eventsCh: events}
}

// StartHTTP starts the OTLP HTTP receiver on the given port.
func (r *Receiver) StartHTTP(ctx context.Context, port uint16) error {
	mux := http.NewServeMux()
	mux.HandleFunc("/v1/traces", r.handleHTTPTraces)
	mux.HandleFunc("/v1/logs", r.handleHTTPLogs)
	mux.HandleFunc("/v1/metrics", r.handleHTTPMetrics)

	r.httpSrv = &http.Server{
		Addr: fmt.Sprintf("0.0.0.0:%d", port),
		// Browser OTLP exporters (e.g. a web app's tracing) preflight with
		// OPTIONS and require CORS headers; without them the POST is blocked.
		Handler: withCORS(mux),
	}
	ln, err := net.Listen("tcp", r.httpSrv.Addr)
	if err != nil {
		return fmt.Errorf("binding OTLP HTTP port %d: %w", port, err)
	}
	go func() {
		_ = r.httpSrv.Serve(ln)
	}()
	go func() {
		<-ctx.Done()
		shutCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = r.httpSrv.Shutdown(shutCtx)
	}()
	return nil
}

// withCORS allows browser-based OTLP exporters to post telemetry: it answers
// CORS preflight (OPTIONS) and reflects the request's Origin and headers. This
// is a local-dev collector, so any origin is permitted.
func withCORS(h http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		if origin := req.Header.Get("Origin"); origin != "" {
			w.Header().Set("Access-Control-Allow-Origin", origin)
			w.Header().Add("Vary", "Origin")
		} else {
			w.Header().Set("Access-Control-Allow-Origin", "*")
		}
		w.Header().Set("Access-Control-Allow-Methods", "POST, OPTIONS")
		reqHeaders := req.Header.Get("Access-Control-Request-Headers")
		if reqHeaders == "" {
			reqHeaders = "Content-Type"
		}
		w.Header().Set("Access-Control-Allow-Headers", reqHeaders)
		w.Header().Set("Access-Control-Max-Age", "86400")
		if req.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		h.ServeHTTP(w, req)
	})
}

// StartGRPC starts the OTLP gRPC receiver on the given port.
func (r *Receiver) StartGRPC(ctx context.Context, port uint16) error {
	ln, err := net.Listen("tcp", fmt.Sprintf("0.0.0.0:%d", port))
	if err != nil {
		return fmt.Errorf("binding OTLP gRPC port %d: %w", port, err)
	}

	r.grpcSrv = grpc.NewServer()
	coltracepb.RegisterTraceServiceServer(r.grpcSrv, &grpcTraceServer{r: r})
	collogspb.RegisterLogsServiceServer(r.grpcSrv, &grpcLogsServer{r: r})
	colmetricspb.RegisterMetricsServiceServer(r.grpcSrv, &grpcMetricsServer{r: r})

	go func() {
		_ = r.grpcSrv.Serve(ln)
	}()
	go func() {
		<-ctx.Done()
		r.grpcSrv.GracefulStop()
	}()
	return nil
}

// --- HTTP handlers ---

// isJSONRequest reports whether the OTLP/HTTP request carries JSON encoding.
func isJSONRequest(req *http.Request) bool {
	return strings.HasPrefix(req.Header.Get("Content-Type"), "application/json")
}

// writeOTLPResponse writes an empty success response in the request's encoding.
func writeOTLPResponse(w http.ResponseWriter, req *http.Request, msg proto.Message) {
	if isJSONRequest(req) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte("{}"))
		return
	}
	w.Header().Set("Content-Type", "application/x-protobuf")
	b, _ := proto.Marshal(msg)
	_, _ = w.Write(b)
}

func (r *Receiver) handleHTTPTraces(w http.ResponseWriter, req *http.Request) {
	body, err := readBody(req)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	// OTLP/JSON encodes trace/span IDs as hex, which plain protojson would
	// mis-decode as base64 — pdata's unmarshaler implements the spec, so we
	// re-encode JSON bodies to protobuf through it.
	if isJSONRequest(req) {
		td, err := (&ptrace.JSONUnmarshaler{}).UnmarshalTraces(body)
		if err != nil {
			http.Error(w, "invalid OTLP JSON: "+err.Error(), http.StatusBadRequest)
			return
		}
		body, err = (&ptrace.ProtoMarshaler{}).MarshalTraces(td)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}
	var pb coltracepb.ExportTraceServiceRequest
	if err := proto.Unmarshal(body, &pb); err != nil {
		http.Error(w, "invalid protobuf", http.StatusBadRequest)
		return
	}
	r.ingestTraces(pb.ResourceSpans)
	writeOTLPResponse(w, req, &coltracepb.ExportTraceServiceResponse{})
}

func (r *Receiver) handleHTTPLogs(w http.ResponseWriter, req *http.Request) {
	body, err := readBody(req)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if isJSONRequest(req) {
		ld, err := (&plog.JSONUnmarshaler{}).UnmarshalLogs(body)
		if err != nil {
			http.Error(w, "invalid OTLP JSON: "+err.Error(), http.StatusBadRequest)
			return
		}
		body, err = (&plog.ProtoMarshaler{}).MarshalLogs(ld)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}
	var pb collogspb.ExportLogsServiceRequest
	if err := proto.Unmarshal(body, &pb); err != nil {
		http.Error(w, "invalid protobuf", http.StatusBadRequest)
		return
	}
	r.ingestLogs(pb.ResourceLogs)
	writeOTLPResponse(w, req, &collogspb.ExportLogsServiceResponse{})
}

func (r *Receiver) handleHTTPMetrics(w http.ResponseWriter, req *http.Request) {
	body, err := readBody(req)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if isJSONRequest(req) {
		md, err := (&pmetric.JSONUnmarshaler{}).UnmarshalMetrics(body)
		if err != nil {
			http.Error(w, "invalid OTLP JSON: "+err.Error(), http.StatusBadRequest)
			return
		}
		body, err = (&pmetric.ProtoMarshaler{}).MarshalMetrics(md)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}
	var pb colmetricspb.ExportMetricsServiceRequest
	if err := proto.Unmarshal(body, &pb); err != nil {
		http.Error(w, "invalid protobuf", http.StatusBadRequest)
		return
	}
	r.ingestMetrics(pb.ResourceMetrics)
	writeOTLPResponse(w, req, &colmetricspb.ExportMetricsServiceResponse{})
}

// --- gRPC servers ---

type grpcTraceServer struct {
	coltracepb.UnimplementedTraceServiceServer
	r *Receiver
}

func (s *grpcTraceServer) Export(_ context.Context, req *coltracepb.ExportTraceServiceRequest) (*coltracepb.ExportTraceServiceResponse, error) {
	s.r.ingestTraces(req.ResourceSpans)
	return &coltracepb.ExportTraceServiceResponse{}, nil
}

type grpcLogsServer struct {
	collogspb.UnimplementedLogsServiceServer
	r *Receiver
}

func (s *grpcLogsServer) Export(_ context.Context, req *collogspb.ExportLogsServiceRequest) (*collogspb.ExportLogsServiceResponse, error) {
	s.r.ingestLogs(req.ResourceLogs)
	return &collogspb.ExportLogsServiceResponse{}, nil
}

type grpcMetricsServer struct {
	colmetricspb.UnimplementedMetricsServiceServer
	r *Receiver
}

func (s *grpcMetricsServer) Export(_ context.Context, req *colmetricspb.ExportMetricsServiceRequest) (*colmetricspb.ExportMetricsServiceResponse, error) {
	s.r.ingestMetrics(req.ResourceMetrics)
	return &colmetricspb.ExportMetricsServiceResponse{}, nil
}

// --- ingestion helpers ---

func (r *Receiver) ingestTraces(rss []*tracepb.ResourceSpans) {
	for _, rs := range rss {
		svcName := resourceAttr(rs.Resource, "service.name")
		for _, ss := range rs.ScopeSpans {
			for _, sp := range ss.Spans {
				stored := convertSpan(sp, svcName)
				r.store.InsertSpan(stored)

				ev := MakeTraceUpdateEvent(TraceUpdatePayload{
					TraceID:    stored.TraceID,
					Service:    stored.ServiceName,
					DurationMs: stored.DurationMs,
					HasError:   stored.Status == SpanStatusError,
				})
				r.publish(ev)
			}
		}
	}
}

func (r *Receiver) ingestLogs(rls []*logspb.ResourceLogs) {
	for _, rl := range rls {
		svcName := resourceAttr(rl.Resource, "service.name")
		for _, sl := range rl.ScopeLogs {
			for _, lr := range sl.LogRecords {
				stored := convertLog(lr, svcName)
				r.store.InsertLog(stored)

				ev := MakeLogRecordEvent(LogRecordPayload{
					TraceID:  stored.TraceID,
					Severity: string(stored.Severity),
					Body:     stored.Body,
					Service:  stored.ServiceName,
				})
				r.publish(ev)
			}
		}
	}
}

func (r *Receiver) ingestMetrics(rms []*metricspb.ResourceMetrics) {
	for _, rm := range rms {
		svcName := resourceAttr(rm.Resource, "service.name")
		for _, sm := range rm.ScopeMetrics {
			for _, m := range sm.Metrics {
				stored := convertMetric(m, svcName)
				if stored == nil {
					continue
				}
				r.store.InsertMetric(*stored)
				ev := MakeMetricUpdateEvent(MetricUpdatePayload{
					Name:    stored.MetricName,
					Value:   stored.Value,
					Service: stored.ServiceName,
				})
				r.publish(ev)
			}
		}
	}
}

func (r *Receiver) publish(ev WSEvent) {
	select {
	case r.eventsCh <- ev:
	default:
	}
}

// --- converters ---

func convertSpan(sp *tracepb.Span, svcName string) StoredSpan {
	traceID := hex.EncodeToString(sp.TraceId)
	spanID := hex.EncodeToString(sp.SpanId)
	var parentSpanID *string
	if len(sp.ParentSpanId) > 0 {
		s := hex.EncodeToString(sp.ParentSpanId)
		parentSpanID = &s
	}

	start := time.Unix(0, int64(sp.StartTimeUnixNano))
	end := time.Unix(0, int64(sp.EndTimeUnixNano))
	durMs := uint64(end.Sub(start).Milliseconds())

	attrs := convertAttrs(sp.Attributes)

	status := SpanStatusUnset
	var statusMsg *string
	if sp.Status != nil {
		switch sp.Status.Code {
		case tracepb.Status_STATUS_CODE_OK:
			status = SpanStatusOk
		case tracepb.Status_STATUS_CODE_ERROR:
			status = SpanStatusError
		}
		if sp.Status.Message != "" {
			m := sp.Status.Message
			statusMsg = &m
		}
	}

	// Infer error from HTTP status if span status is unset.
	if status == SpanStatusUnset {
		for _, attr := range attrs {
			if attr[0] == "http.response.status_code" || attr[0] == "http.status_code" {
				var code int
				fmt.Sscanf(attr[1], "%d", &code)
				if code >= 500 {
					status = SpanStatusError
				} else if code > 0 {
					status = SpanStatusOk
				}
			}
		}
	}

	kind := SpanKindInternal
	switch sp.Kind {
	case tracepb.Span_SPAN_KIND_SERVER:
		kind = SpanKindServer
	case tracepb.Span_SPAN_KIND_CLIENT:
		kind = SpanKindClient
	case tracepb.Span_SPAN_KIND_PRODUCER:
		kind = SpanKindProducer
	case tracepb.Span_SPAN_KIND_CONSUMER:
		kind = SpanKindConsumer
	}

	var events []SpanEvent
	for _, e := range sp.Events {
		events = append(events, SpanEvent{
			Name:       e.Name,
			Timestamp:  time.Unix(0, int64(e.TimeUnixNano)),
			Attributes: convertAttrs(e.Attributes),
		})
	}

	return StoredSpan{
		TraceID:       traceID,
		SpanID:        spanID,
		ParentSpanID:  parentSpanID,
		ServiceName:   svcName,
		OperationName: sp.Name,
		StartTime:     start,
		EndTime:       end,
		DurationMs:    durMs,
		Status:        status,
		StatusMessage: statusMsg,
		Attributes:    attrs,
		Kind:          kind,
		Events:        events,
	}
}

func convertLog(lr *logspb.LogRecord, svcName string) StoredLog {
	ts := time.Unix(0, int64(lr.TimeUnixNano))
	if ts.IsZero() {
		ts = time.Now()
	}

	body := ""
	if lr.Body != nil {
		if sv, ok := lr.Body.Value.(*commonpb.AnyValue_StringValue); ok {
			body = sv.StringValue
		}
	}

	sev := SeverityFromNumber(int32(lr.SeverityNumber))

	var traceID, spanID *string
	if len(lr.TraceId) > 0 {
		s := hex.EncodeToString(lr.TraceId)
		traceID = &s
	}
	if len(lr.SpanId) > 0 {
		s := hex.EncodeToString(lr.SpanId)
		spanID = &s
	}

	return StoredLog{
		Timestamp:   ts,
		ServiceName: svcName,
		Severity:    sev,
		Body:        body,
		TraceID:     traceID,
		SpanID:      spanID,
		Attributes:  convertAttrs(lr.Attributes),
	}
}

func convertMetric(m *metricspb.Metric, svcName string) *StoredMetric {
	ts := time.Now()
	var value float64
	metType := MetricGauge

	switch d := m.Data.(type) {
	case *metricspb.Metric_Gauge:
		metType = MetricGauge
		if len(d.Gauge.DataPoints) > 0 {
			dp := d.Gauge.DataPoints[0]
			ts = time.Unix(0, int64(dp.TimeUnixNano))
			if !ts.IsZero() {
				// valid
			} else {
				ts = time.Now()
			}
			switch v := dp.Value.(type) {
			case *metricspb.NumberDataPoint_AsDouble:
				value = v.AsDouble
			case *metricspb.NumberDataPoint_AsInt:
				value = float64(v.AsInt)
			}
		}
	case *metricspb.Metric_Sum:
		metType = MetricCounter
		if len(d.Sum.DataPoints) > 0 {
			dp := d.Sum.DataPoints[0]
			ts = time.Unix(0, int64(dp.TimeUnixNano))
			switch v := dp.Value.(type) {
			case *metricspb.NumberDataPoint_AsDouble:
				value = v.AsDouble
			case *metricspb.NumberDataPoint_AsInt:
				value = float64(v.AsInt)
			}
		}
	case *metricspb.Metric_Histogram:
		metType = MetricHistogram
		if len(d.Histogram.DataPoints) > 0 {
			dp := d.Histogram.DataPoints[0]
			ts = time.Unix(0, int64(dp.TimeUnixNano))
			if dp.Count > 0 && dp.Sum != nil {
				value = *dp.Sum / float64(dp.Count)
			}
		}
	default:
		return nil
	}

	unit := m.Unit
	var unitPtr *string
	if unit != "" {
		unitPtr = &unit
	}

	return &StoredMetric{
		Timestamp:   ts,
		ServiceName: svcName,
		MetricName:  m.Name,
		MetricType:  metType,
		Value:       value,
		Unit:        unitPtr,
	}
}

func resourceAttr(res interface{ GetAttributes() []*commonpb.KeyValue }, key string) string {
	if res == nil {
		return "unknown"
	}
	for _, kv := range res.GetAttributes() {
		if kv.Key == key {
			if sv, ok := kv.Value.Value.(*commonpb.AnyValue_StringValue); ok {
				return sv.StringValue
			}
		}
	}
	return "unknown"
}

func convertAttrs(attrs []*commonpb.KeyValue) [][2]string {
	out := make([][2]string, 0, len(attrs))
	for _, kv := range attrs {
		var val string
		switch v := kv.Value.Value.(type) {
		case *commonpb.AnyValue_StringValue:
			val = v.StringValue
		case *commonpb.AnyValue_IntValue:
			val = fmt.Sprint(v.IntValue)
		case *commonpb.AnyValue_DoubleValue:
			val = fmt.Sprintf("%g", v.DoubleValue)
		case *commonpb.AnyValue_BoolValue:
			val = fmt.Sprint(v.BoolValue)
		default:
			val = fmt.Sprint(kv.Value)
		}
		out = append(out, [2]string{kv.Key, val})
	}
	return out
}
