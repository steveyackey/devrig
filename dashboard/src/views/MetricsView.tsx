import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { fetchMetrics, fetchStatus, type StoredMetric, type TelemetryEvent } from '../api';

interface MetricsViewProps {
  onEvent?: TelemetryEvent | null;
}

const MetricsView: Component<MetricsViewProps> = (props) => {
  const [metrics, setMetrics] = createSignal<StoredMetric[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [services, setServices] = createSignal<string[]>([]);

  // Filters
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

  // Initial load
  createEffect(() => {
    loadMetrics();
    loadServices();
  });

  // React to WebSocket metric events
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

  const metricTypeColor = (type: string): string => {
    switch (type) {
      case 'Counter': return 'bg-blue-500/20 text-blue-400 border border-blue-500/30';
      case 'Gauge': return 'bg-green-500/20 text-green-400 border border-green-500/30';
      case 'Histogram': return 'bg-purple-500/20 text-purple-400 border border-purple-500/30';
      default: return 'bg-zinc-600/20 text-zinc-400 border border-zinc-600/30';
    }
  };

  return (
    <div class="flex flex-col h-full">
      {/* Header */}
      <div class="px-6 py-4 border-b border-zinc-700/50">
        <h2 class="text-lg font-semibold text-zinc-100">Metrics</h2>
        <p class="text-sm text-zinc-500 mt-0.5">Telemetry metric data points</p>
      </div>

      {/* Filter Bar */}
      <form onSubmit={handleSearch} class="px-6 py-3 border-b border-zinc-700/50 flex items-center gap-3 flex-wrap">
        <div class="flex items-center gap-2">
          <label class="text-xs text-zinc-500 uppercase tracking-wider">Metric Name</label>
          <input
            type="text"
            placeholder="Filter by name..."
            value={filterName()}
            onInput={(e) => setFilterName(e.currentTarget.value)}
            class="bg-zinc-800 border border-zinc-700 rounded-md px-3 py-1.5 text-sm text-zinc-200 focus:outline-none focus:border-blue-500 w-48"
          />
        </div>

        <div class="flex items-center gap-2">
          <label class="text-xs text-zinc-500 uppercase tracking-wider">Service</label>
          <select
            value={filterService()}
            onChange={(e) => setFilterService(e.currentTarget.value)}
            class="bg-zinc-800 border border-zinc-700 rounded-md px-3 py-1.5 text-sm text-zinc-200 focus:outline-none focus:border-blue-500 min-w-[140px]"
          >
            <option value="">All Services</option>
            <For each={services()}>
              {(svc) => <option value={svc}>{svc}</option>}
            </For>
          </select>
        </div>

        <button
          type="submit"
          class="bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium px-4 py-1.5 rounded-md"
        >
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
          class="text-zinc-400 hover:text-zinc-200 text-sm px-3 py-1.5"
        >
          Clear
        </button>

        <div class="ml-auto text-xs text-zinc-600">
          {metrics().length} metric{metrics().length !== 1 ? 's' : ''}
        </div>
      </form>

      {/* Table */}
      <div class="flex-1 overflow-auto">
        <Show when={error()}>
          <div class="px-6 py-8 text-center">
            <p class="text-red-400 text-sm">{error()}</p>
            <button
              onClick={() => { setLoading(true); loadMetrics(); }}
              class="mt-2 text-blue-400 hover:text-blue-300 text-sm"
            >
              Retry
            </button>
          </div>
        </Show>

        <Show when={loading() && metrics().length === 0}>
          <div class="px-6 py-12 text-center text-zinc-500 text-sm">
            Loading metrics...
          </div>
        </Show>

        <Show when={!loading() && !error() && metrics().length === 0}>
          <div class="px-6 py-12 text-center text-zinc-500 text-sm">
            No metrics found. Adjust filters or wait for new data.
          </div>
        </Show>

        <Show when={metrics().length > 0}>
          <table class="w-full">
            <thead class="sticky top-0 z-10">
              <tr class="bg-zinc-800/90 backdrop-blur text-xs text-zinc-500 uppercase tracking-wider">
                <th class="text-left px-6 py-2.5 font-medium">Time</th>
                <th class="text-left px-4 py-2.5 font-medium">Service</th>
                <th class="text-left px-4 py-2.5 font-medium">Metric Name</th>
                <th class="text-left px-4 py-2.5 font-medium">Type</th>
                <th class="text-right px-4 py-2.5 font-medium">Value</th>
                <th class="text-left px-6 py-2.5 font-medium">Unit</th>
              </tr>
            </thead>
            <tbody>
              <For each={metrics()}>
                {(metric) => (
                  <tr class="border-b border-zinc-800/30 hover:bg-zinc-800/40">
                    <td class="px-6 py-2.5 text-xs font-mono text-zinc-500 whitespace-nowrap">
                      {formatTime(metric.timestamp)}
                    </td>
                    <td class="px-4 py-2.5 text-sm text-zinc-400">
                      {metric.service_name}
                    </td>
                    <td class="px-4 py-2.5 text-sm text-zinc-300 font-mono">
                      {metric.metric_name}
                    </td>
                    <td class="px-4 py-2.5">
                      <span class={`inline-block text-xs font-medium px-2 py-0.5 rounded ${metricTypeColor(metric.metric_type)}`}>
                        {metric.metric_type}
                      </span>
                    </td>
                    <td class="px-4 py-2.5 text-right text-sm font-mono text-zinc-200">
                      {formatValue(metric.value)}
                    </td>
                    <td class="px-6 py-2.5 text-sm text-zinc-500">
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
