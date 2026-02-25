// ---- Type definitions ----

export interface TraceSummary {
  trace_id: string;
  services: string[];
  root_operation: string;
  duration_ms: number;
  span_count: number;
  has_error: boolean;
  start_time: string;
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
  | { type: "TraceUpdate"; payload: { trace_id: string; service: string; duration_ms: number; has_error: boolean } }
  | { type: "LogRecord"; payload: { trace_id: string | null; severity: string; body: string; service: string } }
  | { type: "MetricUpdate"; payload: { name: string; value: number; service: string } }
  | { type: "ServiceStatusChange"; payload: { service: string; status: string } };

// ---- API functions ----

const BASE_URL = window.location.origin;

async function fetchJson<T>(url: string): Promise<T> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`API error: ${response.status} ${response.statusText}`);
  }
  return response.json();
}

export interface TracesParams {
  service?: string;
  status?: string;
  min_duration_ms?: number;
  limit?: number;
}

export function fetchTraces(params: TracesParams = {}): Promise<TraceSummary[]> {
  const query = new URLSearchParams();
  if (params.service) query.set('service', params.service);
  if (params.status) query.set('status', params.status);
  if (params.min_duration_ms !== undefined && params.min_duration_ms > 0) {
    query.set('min_duration_ms', String(params.min_duration_ms));
  }
  if (params.limit !== undefined) query.set('limit', String(params.limit));
  const qs = query.toString();
  return fetchJson<TraceSummary[]>(`${BASE_URL}/api/traces${qs ? '?' + qs : ''}`);
}

export function fetchTrace(id: string): Promise<TraceDetailResponse> {
  return fetchJson<TraceDetailResponse>(`${BASE_URL}/api/traces/${encodeURIComponent(id)}`);
}

export function fetchRelated(id: string): Promise<RelatedResponse> {
  return fetchJson<RelatedResponse>(`${BASE_URL}/api/traces/${encodeURIComponent(id)}/related`);
}

export interface LogsParams {
  service?: string;
  severity?: string;
  search?: string;
  trace_id?: string;
  limit?: number;
  /** Filter by log source: "process" (stdout+stderr), "stdout", "stderr", "docker", "otlp" */
  source?: string;
}

export function fetchLogs(params: LogsParams = {}): Promise<StoredLog[]> {
  const query = new URLSearchParams();
  if (params.service) query.set('service', params.service);
  if (params.severity) query.set('severity', params.severity);
  if (params.search) query.set('search', params.search);
  if (params.trace_id) query.set('trace_id', params.trace_id);
  if (params.source) query.set('source', params.source);
  if (params.limit !== undefined) query.set('limit', String(params.limit));
  const qs = query.toString();
  return fetchJson<StoredLog[]>(`${BASE_URL}/api/logs${qs ? '?' + qs : ''}`);
}

export interface MetricsParams {
  name?: string;
  service?: string;
  limit?: number;
}

export function fetchMetrics(params: MetricsParams = {}): Promise<StoredMetric[]> {
  const query = new URLSearchParams();
  if (params.name) query.set('name', params.name);
  if (params.service) query.set('service', params.service);
  if (params.limit !== undefined) query.set('limit', String(params.limit));
  const qs = query.toString();
  return fetchJson<StoredMetric[]>(`${BASE_URL}/api/metrics${qs ? '?' + qs : ''}`);
}

export function fetchMetricSeries(
  name: string,
  service?: string,
  since?: string,
): Promise<MetricSeriesResponse> {
  const query = new URLSearchParams();
  query.set('name', name);
  if (service) query.set('service', service);
  if (since) query.set('since', since);
  return fetchJson<MetricSeriesResponse>(`${BASE_URL}/api/metrics/series?${query.toString()}`);
}

export function fetchStatus(): Promise<StatusResponse> {
  return fetchJson<StatusResponse>(`${BASE_URL}/api/status`);
}

// ---- Services API ----

export interface ServiceInfo {
  name: string;
  port: number | null;
  kind: string;
  port_auto: boolean;
  protocol?: string;
  phase?: string;
  exit_code?: number | null;
}

export function fetchServices(): Promise<ServiceInfo[]> {
  return fetchJson<ServiceInfo[]>(`${BASE_URL}/api/services`);
}

// ---- Cluster API ----

export interface RegistryInfo {
  name: string;
  port: number;
}

export interface DeployedServiceInfo {
  name: string;
  image_tag: string;
  last_deployed: string;
}

export interface AddonInfo {
  name: string;
  addon_type: string;
  namespace: string;
  installed_at: string;
}

export interface ClusterResponse {
  cluster_name: string;
  kubeconfig_path: string;
  registry: RegistryInfo | null;
  deployed_services: DeployedServiceInfo[];
  addons: AddonInfo[];
}

export function fetchCluster(): Promise<ClusterResponse | null> {
  return fetchJson<ClusterResponse | null>(`${BASE_URL}/api/cluster`);
}

// ---- Config API ----

export interface ConfigResponse {
  content: string;
  hash: string;
}

export interface ConfigErrorResponse {
  error: string;
}

export async function fetchConfig(): Promise<ConfigResponse> {
  return fetchJson<ConfigResponse>(`${BASE_URL}/api/config`);
}

export async function updateConfig(content: string, hash: string): Promise<ConfigResponse> {
  const response = await fetch(`${BASE_URL}/api/config`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content, hash }),
  });
  if (!response.ok) {
    const err = await response.json() as ConfigErrorResponse;
    throw new Error(err.error || `API error: ${response.status}`);
  }
  return response.json();
}

export function connectWebSocket(onEvent: (event: TelemetryEvent) => void, onOpen?: () => void): () => void {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const wsUrl = `${protocol}//${window.location.host}/ws`;

  let ws: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let closed = false;

  function connect() {
    if (closed) return;

    ws = new WebSocket(wsUrl);

    ws.onopen = () => {
      console.log('[devrig] WebSocket connected');
      onOpen?.();
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as TelemetryEvent;
        onEvent(data);
      } catch (err) {
        console.warn('[devrig] Failed to parse WebSocket message:', err);
      }
    };

    ws.onclose = () => {
      if (!closed) {
        console.log('[devrig] WebSocket disconnected, reconnecting in 3s...');
        reconnectTimer = setTimeout(connect, 3000);
      }
    };

    ws.onerror = (err) => {
      console.warn('[devrig] WebSocket error:', err);
      ws?.close();
    };
  }

  connect();

  return () => {
    closed = true;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    ws?.close();
  };
}
