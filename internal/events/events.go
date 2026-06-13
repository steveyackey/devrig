// Package events defines telemetry events and a broadcast channel for
// distributing them to multiple consumers (dashboard WebSocket, log UI, etc.).
package events

import "sync"

// TelemetryEvent is the discriminated union of events emitted by the orchestrator.
type TelemetryEvent struct {
	Kind ServiceStatusKind

	// ServiceStatusChange fields
	Service string
	Status  string

	// LogRecord fields (also set Service)
	LogBody     string
	LogSeverity string
	TraceID     string
	SpanID      string

	// TraceUpdate / MetricUpdate carry their data as raw JSON to be forwarded
	// to WebSocket clients unchanged.
	RawJSON []byte
}

type ServiceStatusKind int

const (
	KindServiceStatusChange ServiceStatusKind = iota
	KindLogRecord
	KindTraceUpdate
	KindMetricUpdate
)

// Broadcaster distributes events to an unbounded set of subscribers.
// Slow subscribers are not blocked — they receive the event asynchronously
// via a buffered channel.
type Broadcaster struct {
	mu   sync.Mutex
	subs []chan TelemetryEvent
}

// Subscribe returns a channel that will receive future events. The caller owns
// the channel and must drain it promptly; cap controls the buffer depth.
func (b *Broadcaster) Subscribe(cap int) <-chan TelemetryEvent {
	ch := make(chan TelemetryEvent, cap)
	b.mu.Lock()
	b.subs = append(b.subs, ch)
	b.mu.Unlock()
	return ch
}

// Unsubscribe removes a previously subscribed channel and closes it.
func (b *Broadcaster) Unsubscribe(ch <-chan TelemetryEvent) {
	b.mu.Lock()
	defer b.mu.Unlock()
	for i, s := range b.subs {
		if s == ch {
			b.subs = append(b.subs[:i], b.subs[i+1:]...)
			close(s)
			return
		}
	}
}

// Send delivers an event to all current subscribers, dropping it for any
// subscriber whose buffer is full (non-blocking).
func (b *Broadcaster) Send(ev TelemetryEvent) {
	b.mu.Lock()
	defer b.mu.Unlock()
	for _, ch := range b.subs {
		select {
		case ch <- ev:
		default:
		}
	}
}

// Close closes all subscriber channels.
func (b *Broadcaster) Close() {
	b.mu.Lock()
	defer b.mu.Unlock()
	for _, ch := range b.subs {
		close(ch)
	}
	b.subs = nil
}

// LogLine is a structured log line emitted by a supervised service.
type LogLine struct {
	Service   string
	Text      string
	IsStderr  bool
	Level     string
}

// DetectLogLevel infers a log severity from common patterns in the log text.
func DetectLogLevel(text string) string {
	for _, ch := range []struct {
		words []string
		level string
	}{
		{[]string{"ERROR", "error", "FATAL", "fatal", "PANIC", "panic", "CRIT", "crit"}, "error"},
		{[]string{"WARN", "warn", "WARNING", "warning"}, "warn"},
		{[]string{"DEBUG", "debug", "TRACE", "trace"}, "debug"},
	} {
		for _, w := range ch.words {
			if containsWord(text, w) {
				return ch.level
			}
		}
	}
	return "info"
}

func containsWord(s, word string) bool {
	// Cheap contains check — not full word-boundary, but matches common formats.
	for i := 0; i+len(word) <= len(s); i++ {
		if s[i:i+len(word)] == word {
			return true
		}
	}
	return false
}
