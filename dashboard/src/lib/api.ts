// Re-export everything from the top-level api module so that
// imports from 'lib/api' work alongside the existing '../api' imports.
export {
	fetchTraces,
	fetchTrace,
	fetchRelated,
	fetchLogs,
	fetchMetrics,
	fetchMetricSeries,
	fetchServices,
	fetchStatus,
	fetchConfig,
	updateConfig,
	connectWebSocket,
} from "../api";

export type {
	TraceSummary,
	StoredSpan,
	TraceDetailResponse,
	StoredLog,
	StoredMetric,
	MetricSeries,
	MetricSeriesResponse,
	StatusResponse,
	RelatedResponse,
	TelemetryEvent,
	TracesParams,
	LogsParams,
	MetricsParams,
	ServiceInfo,
	ConfigResponse,
	ConfigErrorResponse,
} from "../api";
