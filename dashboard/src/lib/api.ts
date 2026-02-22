// Re-export everything from the top-level api module so that
// imports from 'lib/api' work alongside the existing '../api' imports.
export {
  fetchTraces,
  fetchTrace,
  fetchRelated,
  fetchLogs,
  fetchMetrics,
  fetchStatus,
  connectWebSocket,
} from '../api';

export type {
  TraceSummary,
  StoredSpan,
  TraceDetailResponse,
  StoredLog,
  StoredMetric,
  StatusResponse,
  RelatedResponse,
  TelemetryEvent,
  TracesParams,
  LogsParams,
  MetricsParams,
} from '../api';
