/**
 * Simple reactive state store for telemetry data.
 *
 * Built on SolidJS signals so it integrates naturally with the rest of
 * the dashboard. Components can read from the store's signals and will
 * automatically re-render when the data changes.
 *
 * Usage:
 *   import { telemetryStore } from '../lib/store';
 *
 *   // In a component:
 *   const traces = telemetryStore.traces();
 *   telemetryStore.addTrace(newTrace);
 */

import { createSignal, batch } from 'solid-js';
import type {
  TraceSummary,
  StoredLog,
  StoredMetric,
  StatusResponse,
  TelemetryEvent,
} from '../api';

// ---------------------------------------------------------------------------
// Store shape
// ---------------------------------------------------------------------------

export interface TelemetryStore {
  // Signals (read-only accessors)
  traces: () => TraceSummary[];
  logs: () => StoredLog[];
  metrics: () => StoredMetric[];
  status: () => StatusResponse | null;
  services: () => string[];
  connected: () => boolean;

  // Mutators
  setTraces: (traces: TraceSummary[]) => void;
  setLogs: (logs: StoredLog[]) => void;
  setMetrics: (metrics: StoredMetric[]) => void;
  setStatus: (status: StatusResponse) => void;
  setConnected: (connected: boolean) => void;

  /** Convenience: push a single trace summary (deduplicates by trace_id). */
  addTrace: (trace: TraceSummary) => void;

  /** Convenience: push a single log record, keeping the list bounded. */
  addLog: (log: StoredLog) => void;

  /** Convenience: push a single metric data point, keeping the list bounded. */
  addMetric: (metric: StoredMetric) => void;

  /** Process an incoming WebSocket TelemetryEvent and update state accordingly. */
  handleEvent: (event: TelemetryEvent) => void;

  /** Reset all store data back to initial empty state. */
  reset: () => void;
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/** Maximum number of log entries to keep in the store. */
const MAX_LOGS = 1000;

/** Maximum number of metric data points to keep in the store. */
const MAX_METRICS = 1000;

/** Maximum number of traces to keep in the store. */
const MAX_TRACES = 500;

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

export function createTelemetryStore(): TelemetryStore {
  const [traces, setTraces] = createSignal<TraceSummary[]>([]);
  const [logs, setLogs] = createSignal<StoredLog[]>([]);
  const [metrics, setMetrics] = createSignal<StoredMetric[]>([]);
  const [status, setStatus] = createSignal<StatusResponse | null>(null);
  const [services, setServices] = createSignal<string[]>([]);
  const [connected, setConnected] = createSignal(false);

  function addTrace(trace: TraceSummary) {
    setTraces((prev) => {
      const filtered = prev.filter((t) => t.trace_id !== trace.trace_id);
      const next = [trace, ...filtered];
      return next.slice(0, MAX_TRACES);
    });
  }

  function addLog(log: StoredLog) {
    setLogs((prev) => {
      const next = [log, ...prev];
      return next.slice(0, MAX_LOGS);
    });
  }

  function addMetric(metric: StoredMetric) {
    setMetrics((prev) => {
      const next = [metric, ...prev];
      return next.slice(0, MAX_METRICS);
    });
  }

  function handleEvent(event: TelemetryEvent) {
    switch (event.type) {
      case 'TraceUpdate': {
        const p = event.payload;
        addTrace({
          trace_id: p.trace_id,
          services: [p.service],
          root_operation: '',
          duration_ms: p.duration_ms,
          span_count: 0,
          has_error: p.has_error,
          start_time: new Date().toISOString(),
        });
        break;
      }
      case 'LogRecord': {
        const p = event.payload;
        addLog({
          record_id: Date.now(),
          timestamp: new Date().toISOString(),
          service_name: p.service,
          severity: p.severity as StoredLog['severity'],
          body: p.body,
          trace_id: p.trace_id,
          span_id: null,
          attributes: [],
        });
        break;
      }
      case 'MetricUpdate': {
        const p = event.payload;
        addMetric({
          record_id: Date.now(),
          timestamp: new Date().toISOString(),
          service_name: p.service,
          metric_name: p.name,
          metric_type: 'Gauge',
          value: p.value,
          attributes: [],
          unit: null,
        });
        break;
      }
      case 'ServiceStatusChange': {
        const p = event.payload;
        setServices((prev) => {
          if (!prev.includes(p.service)) {
            return [...prev, p.service];
          }
          return prev;
        });
        break;
      }
    }
  }

  function reset() {
    batch(() => {
      setTraces([]);
      setLogs([]);
      setMetrics([]);
      setStatus(null);
      setServices([]);
      setConnected(false);
    });
  }

  return {
    traces,
    logs,
    metrics,
    status,
    services,
    connected,

    setTraces,
    setLogs,
    setMetrics,
    setStatus: (s: StatusResponse) => {
      setStatus(s);
      setServices(s.services);
    },
    setConnected,

    addTrace,
    addLog,
    addMetric,
    handleEvent,
    reset,
  };
}

/**
 * Singleton store instance for the application.
 * Import this in components that need access to telemetry state.
 */
export const telemetryStore = createTelemetryStore();
