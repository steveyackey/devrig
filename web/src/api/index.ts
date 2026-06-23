// ---- Type definitions ----

export interface TraceSummary {
  trace_id: string;
  services: string[];
  root_operation: string;
  duration_ms: number;
  span_count: number;
  has_error: boolean;
  start_time: string;
  http_status?: number;
}

export interface StoredSpanEvent {
  name: string;
  timestamp: string;
  attributes: [string, string][];
}

export interface StoredSpan {
  record_id: number;
  trace_id: string;
  span_id: string;
  parent_span_id: string | null;
  service_name: string;
  operation_name: string;
  start_time: string;
  end_time: string;
  duration_ms: number;
  status: "Ok" | "Error" | "Unset";
  status_message: string | null;
  attributes: [string, string][];
  kind: "Internal" | "Server" | "Client" | "Producer" | "Consumer";
  events: StoredSpanEvent[];
}

export interface TraceDetailResponse {
  trace_id: string;
  spans: StoredSpan[];
}

export interface StoredLog {
  record_id: number;
  timestamp: string;
  service_name: string;
  severity: "Trace" | "Debug" | "Info" | "Warn" | "Error" | "Fatal";
  body: string;
  trace_id: string | null;
  span_id: string | null;
  attributes: [string, string][];
}

export interface StoredMetric {
  record_id: number;
  timestamp: string;
  service_name: string;
  metric_name: string;
  metric_type: "Gauge" | "Counter" | "Histogram";
  value: number;
  attributes: [string, string][];
  unit: string | null;
}

export interface MetricSeriesPoint {
  t: number;
  v: number;
}

export interface MetricSeries {
  metric_name: string;
  service_name: string;
  metric_type: "Gauge" | "Counter" | "Histogram";
  unit: string | null;
  points: MetricSeriesPoint[];
}

export interface MetricSeriesResponse {
  series: MetricSeries[];
}

export interface StatusResponse {
  span_count: number;
  log_count: number;
  metric_count: number;
  services: string[];
  trace_count: number;
}

export interface RelatedResponse {
  logs: StoredLog[];
  metrics: StoredMetric[];
}

export type TelemetryEvent =
  | {
      type: "TraceUpdate";
      payload: { trace_id: string; service: string; duration_ms: number; has_error: boolean };
    }
  | {
      type: "LogRecord";
      payload: { trace_id: string | null; severity: string; body: string; service: string };
    }
  | { type: "MetricUpdate"; payload: { name: string; value: number; service: string } }
  | { type: "ServiceStatusChange"; payload: { service: string; status: string } };

export interface ServiceInfo {
  name: string;
  port: number | null;
  kind?: string;
  port_auto?: boolean;
  protocol?: string;
  phase?: string;
  exit_code?: number | null;
  addon_type?: string;
  url?: string;
}

export interface ClusterResponse {
  cluster_name: string;
  kubeconfig_path: string;
  registry_name?: string;
  registry_port?: number;
  // Maps keyed by name (matches the Go ClusterState shape). May be null when empty.
  deployed_services: Record<string, { image_tag: string; last_deployed: string }> | null;
  installed_addons: Record<string, { addon_type: string; namespace: string; installed_at: string }> | null;
  k3d_version?: string;
}

export interface ConfigResponse {
  content: string;
  hash: string;
}

// ---- API functions ----

async function fetchJson<T>(url: string): Promise<T> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`API error: ${response.status} ${response.statusText}`);
  return response.json() as Promise<T>;
}

export interface TracesParams {
  service?: string;
  status?: string;
  min_duration_ms?: number;
  search?: string;
  limit?: number;
}

export function fetchTraces(params: TracesParams = {}): Promise<TraceSummary[]> {
  const q = new URLSearchParams();
  if (params.service) q.set("service", params.service);
  if (params.status) q.set("status", params.status);
  if (params.min_duration_ms !== undefined && params.min_duration_ms > 0)
    q.set("min_duration_ms", String(params.min_duration_ms));
  if (params.search) q.set("search", params.search);
  if (params.limit !== undefined) q.set("limit", String(params.limit));
  const qs = q.toString();
  return fetchJson<TraceSummary[]>(`/api/traces${qs ? "?" + qs : ""}`);
}

export function fetchTrace(id: string): Promise<TraceDetailResponse> {
  return fetchJson<TraceDetailResponse>(`/api/traces/${encodeURIComponent(id)}`);
}

export function fetchRelated(id: string): Promise<RelatedResponse> {
  return fetchJson<RelatedResponse>(`/api/traces/${encodeURIComponent(id)}/related`);
}

export interface LogsParams {
  service?: string;
  severity?: string;
  search?: string;
  source?: string;
  trace_id?: string;
  limit?: number;
}

export function fetchLogs(params: LogsParams = {}): Promise<StoredLog[]> {
  const q = new URLSearchParams();
  if (params.service) q.set("service", params.service);
  if (params.severity) q.set("severity", params.severity);
  if (params.search) q.set("search", params.search);
  if (params.source) q.set("source", params.source);
  if (params.trace_id) q.set("trace_id", params.trace_id);
  if (params.limit !== undefined) q.set("limit", String(params.limit));
  const qs = q.toString();
  return fetchJson<StoredLog[]>(`/api/logs${qs ? "?" + qs : ""}`);
}

export interface MetricsParams {
  name?: string;
  metric_type?: string;
  service?: string;
  limit?: number;
}

export function fetchMetrics(params: MetricsParams = {}): Promise<StoredMetric[]> {
  const q = new URLSearchParams();
  if (params.name) q.set("metric_name", params.name);
  if (params.metric_type) q.set("metric_type", params.metric_type);
  if (params.service) q.set("service", params.service);
  if (params.limit !== undefined) q.set("limit", String(params.limit));
  const qs = q.toString();
  return fetchJson<StoredMetric[]>(`/api/metrics${qs ? "?" + qs : ""}`);
}

export function fetchMetricSeries(
  name: string,
  service?: string,
  since?: string,
): Promise<MetricSeriesResponse> {
  const q = new URLSearchParams();
  q.set("metric_name", name);
  if (service) q.set("service", service);
  if (since) q.set("since", since);
  return fetchJson<MetricSeriesResponse>(`/api/metrics/series?${q.toString()}`);
}

export function fetchStatus(): Promise<StatusResponse> {
  return fetchJson<StatusResponse>("/api/status");
}

export function fetchServices(): Promise<ServiceInfo[]> {
  return fetchJson<ServiceInfo[]>("/api/services");
}

export function fetchCluster(): Promise<ClusterResponse | null> {
  return fetchJson<ClusterResponse | null>("/api/cluster");
}

export function fetchConfig(): Promise<ConfigResponse> {
  return fetchJson<ConfigResponse>("/api/config");
}

export async function updateConfig(content: string, hash: string): Promise<ConfigResponse> {
  const response = await fetch("/api/config", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ content, hash }),
  });
  if (!response.ok) {
    const err = (await response.json()) as { error?: string };
    throw new Error(err.error ?? `API error: ${response.status}`);
  }
  return response.json() as Promise<ConfigResponse>;
}
