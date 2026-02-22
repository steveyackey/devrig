import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { fetchMetrics, fetchStatus, type StoredMetric, type TelemetryEvent } from '../api';
import { Badge, Skeleton } from '../components/ui';

interface MetricsViewProps {
  onEvent?: TelemetryEvent | null;
}

const MetricsView: Component<MetricsViewProps> = (props) => {
  const [metrics, setMetrics] = createSignal<StoredMetric[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [services, setServices] = createSignal<string[]>([]);

  const [filterName, setFilterName] = createSignal('');
  const [filterService, setFilterService] = createSignal('');

  const loadMetrics = async () => {
    try {
      setError(null);
      const data = await fetchMetrics({
        name: filterName() || undefined,
        service: filterService() || undefined,
        limit: 200,
      });
      setMetrics(data);
    } catch (err: any) {
      setError(err.message || 'Failed to load metrics');
    } finally {
      setLoading(false);
    }
  };

  const loadServices = async () => {
    try {
      const status = await fetchStatus();
      setServices(status.services);
    } catch {
      // non-critical
    }
  };

  createEffect(() => {
    loadMetrics();
    loadServices();
  });

  createEffect(() => {
    const event = props.onEvent;
    if (event && event.type === 'MetricUpdate') {
      loadMetrics();
    }
  });

  const handleSearch = (e: Event) => {
    e.preventDefault();
    setLoading(true);
    loadMetrics();
  };

  const formatTime = (iso: string): string => {
    try {
      const d = new Date(iso);
      return d.toLocaleTimeString(undefined, {
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
      });
    } catch {
      return iso;
    }
  };

  const formatValue = (value: number): string => {
    if (Number.isInteger(value)) return value.toLocaleString();
    return value.toFixed(4);
  };

  const metricTypeVariant = (type: string) => {
    switch (type) {
      case 'Counter': return 'counter' as const;
      case 'Gauge': return 'gauge' as const;
      case 'Histogram': return 'histogram' as const;
      default: return 'default' as const;
    }
  };

  return (
    <div data-testid="metrics-view" class="flex flex-col h-full">
      <div class="px-6 py-5 border-b border-border">
        <h2 class="text-lg font-semibold text-text-primary">Metrics</h2>
        <p class="text-sm text-text-muted mt-0.5">Telemetry metric data points</p>
      </div>

      <form onSubmit={handleSearch} class="px-6 py-4 border-b border-border flex items-center gap-3 flex-wrap">
        <div class="flex items-center gap-2">
          <label class="text-xs text-text-muted uppercase tracking-wider">Metric Name</label>
          <input
            type="text"
            placeholder="Filter by name..."
            value={filterName()}
            onInput={(e) => setFilterName(e.currentTarget.value)}
            class="bg-surface-2 border border-border rounded-md px-3 py-1.5 text-sm text-text-primary focus:outline-none focus:border-accent w-48"
          />
        </div>

        <div class="flex items-center gap-2">
          <label class="text-xs text-text-muted uppercase tracking-wider">Service</label>
          <select
            value={filterService()}
            onChange={(e) => setFilterService(e.currentTarget.value)}
            class="bg-surface-2 border border-border rounded-md px-3 py-1.5 text-sm text-text-primary focus:outline-none focus:border-accent min-w-[140px]"
          >
            <option value="">All Services</option>
            <For each={services()}>
              {(svc) => <option value={svc}>{svc}</option>}
            </For>
          </select>
        </div>

        <button type="submit" class="bg-accent hover:bg-accent-hover text-white text-sm font-medium px-4 py-1.5 rounded-md transition-colors">
          Search
        </button>

        <button
          type="button"
          onClick={() => {
            setFilterName('');
            setFilterService('');
            setLoading(true);
            loadMetrics();
          }}
          class="text-text-secondary hover:text-text-primary text-sm px-3 py-1.5"
        >
          Clear
        </button>

        <div data-testid="metrics-count" class="ml-auto text-xs text-text-muted">
          {metrics().length} metric{metrics().length !== 1 ? 's' : ''}
        </div>
      </form>

      <div class="flex-1 overflow-auto">
        <Show when={error()}>
          <div class="px-6 py-8 text-center">
            <p class="text-error text-sm">{error()}</p>
            <button onClick={() => { setLoading(true); loadMetrics(); }} class="mt-2 text-accent hover:text-accent-hover text-sm">Retry</button>
          </div>
        </Show>

        <Show when={loading() && metrics().length === 0}>
          <div class="px-6 py-4 space-y-2">
            <For each={[1, 2, 3, 4, 5]}>{() => <Skeleton class="h-8 w-full" />}</For>
          </div>
        </Show>

        <Show when={!loading() || metrics().length > 0}>
          <table class="w-full">
            <thead class="sticky top-0 z-10">
              <tr class="bg-surface-2/90 backdrop-blur text-xs text-text-muted uppercase tracking-wider">
                <th class="text-left px-6 py-2.5 font-medium">Time</th>
                <th class="text-left px-4 py-2.5 font-medium">Service</th>
                <th class="text-left px-4 py-2.5 font-medium">Metric Name</th>
                <th class="text-left px-4 py-2.5 font-medium">Type</th>
                <th class="text-right px-4 py-2.5 font-medium">Value</th>
                <th class="text-left px-6 py-2.5 font-medium">Unit</th>
              </tr>
            </thead>
            <tbody>
              <Show when={!loading() && !error() && metrics().length === 0}>
                <tr><td colspan="6" class="px-6 py-12 text-center text-text-muted text-sm">No metrics found. Adjust filters or wait for new data.</td></tr>
              </Show>
              <For each={metrics()}>
                {(metric) => (
                  <tr data-testid="metric-row" class="border-b border-border/30 hover:bg-surface-2/40 animate-fade-in">
                    <td class="px-6 py-2.5 text-xs font-mono text-text-muted whitespace-nowrap">
                      {formatTime(metric.timestamp)}
                    </td>
                    <td class="px-4 py-2.5 text-sm text-text-muted">
                      {metric.service_name}
                    </td>
                    <td class="px-4 py-2.5">
                      <span data-testid="metric-name" class="text-sm text-text-secondary font-mono">{metric.metric_name}</span>
                    </td>
                    <td class="px-4 py-2.5">
                      <Badge data-testid="metric-type-badge" variant={metricTypeVariant(metric.metric_type)}>
                        {metric.metric_type}
                      </Badge>
                    </td>
                    <td class="px-4 py-2.5 text-right">
                      <span data-testid="metric-value" class="text-sm font-mono text-text-primary">{formatValue(metric.value)}</span>
                    </td>
                    <td class="px-6 py-2.5 text-sm text-text-muted">
                      {metric.unit ?? '-'}
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </Show>
      </div>
    </div>
  );
};

export default MetricsView;
