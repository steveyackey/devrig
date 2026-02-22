import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { fetchTraces, fetchStatus, type TraceSummary, type TelemetryEvent } from '../api';
import { Badge, Skeleton } from '../components/ui';

interface TracesViewProps {
  onEvent?: TelemetryEvent | null;
}

const TracesView: Component<TracesViewProps> = (props) => {
  const [traces, setTraces] = createSignal<TraceSummary[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [services, setServices] = createSignal<string[]>([]);

  // Filters
  const [filterService, setFilterService] = createSignal('');
  const [filterStatus, setFilterStatus] = createSignal('');
  const [filterMinDuration, setFilterMinDuration] = createSignal('');

  const loadTraces = async () => {
    try {
      setError(null);
      const minDur = parseInt(filterMinDuration(), 10);
      const data = await fetchTraces({
        service: filterService() || undefined,
        status: filterStatus() || undefined,
        min_duration_ms: isNaN(minDur) ? undefined : minDur,
        limit: 100,
      });
      setTraces(data);
    } catch (err: any) {
      setError(err.message || 'Failed to load traces');
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
    loadTraces();
    loadServices();
    const interval = setInterval(loadTraces, 10000);
    onCleanup(() => clearInterval(interval));
  });

  createEffect(() => {
    const event = props.onEvent;
    if (event && event.type === 'TraceUpdate') {
      loadTraces();
    }
  });

  const handleSearch = (e: Event) => {
    e.preventDefault();
    setLoading(true);
    loadTraces();
  };

  const formatDuration = (ms: number): string => {
    if (ms < 1) return '<1ms';
    if (ms < 1000) return `${ms.toFixed(1)}ms`;
    return `${(ms / 1000).toFixed(2)}s`;
  };

  const truncateId = (id: string): string => {
    if (id.length <= 16) return id;
    return id.slice(0, 8) + '...' + id.slice(-4);
  };

  const formatTime = (iso: string): string => {
    try {
      const d = new Date(iso);
      return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    } catch {
      return iso;
    }
  };

  return (
    <div data-testid="traces-view" class="flex flex-col h-full">
      {/* Header */}
      <div class="px-7 py-6 border-b border-border">
        <h2 class="text-xl font-semibold text-text-primary">Traces</h2>
        <p class="text-sm text-text-secondary mt-0.5">Distributed trace overview</p>
      </div>

      {/* Filter Bar */}
      <form onSubmit={handleSearch} class="px-7 py-5 border-b border-border flex items-center gap-4 flex-wrap">
        <div class="flex items-center gap-2">
          <label class="text-xs text-text-secondary uppercase tracking-wider">Service</label>
          <select
            value={filterService()}
            onChange={(e) => setFilterService(e.currentTarget.value)}
            class="bg-surface-2 border border-border rounded-md px-3.5 py-2 text-sm text-text-primary focus:outline-none focus:border-accent min-w-[140px]"
          >
            <option value="">All Services</option>
            <For each={services()}>
              {(svc) => <option value={svc}>{svc}</option>}
            </For>
          </select>
        </div>

        <div class="flex items-center gap-2">
          <label class="text-xs text-text-secondary uppercase tracking-wider">Status</label>
          <select
            value={filterStatus()}
            onChange={(e) => setFilterStatus(e.currentTarget.value)}
            class="bg-surface-2 border border-border rounded-md px-3.5 py-2 text-sm text-text-primary focus:outline-none focus:border-accent min-w-[120px]"
          >
            <option value="">All</option>
            <option value="Ok">Ok</option>
            <option value="Error">Error</option>
          </select>
        </div>

        <div class="flex items-center gap-2">
          <label class="text-xs text-text-secondary uppercase tracking-wider">Min Duration</label>
          <input
            type="number"
            placeholder="ms"
            value={filterMinDuration()}
            onInput={(e) => setFilterMinDuration(e.currentTarget.value)}
            class="bg-surface-2 border border-border rounded-md px-3.5 py-2 text-sm text-text-primary focus:outline-none focus:border-accent w-24"
          />
        </div>

        <button
          type="submit"
          class="bg-accent hover:bg-accent-hover text-white text-sm font-medium px-5 py-2 rounded-md transition-colors"
        >
          Search
        </button>

        <button
          type="button"
          onClick={() => {
            setFilterService('');
            setFilterStatus('');
            setFilterMinDuration('');
            setLoading(true);
            loadTraces();
          }}
          class="text-text-secondary hover:text-text-primary text-sm px-3.5 py-2"
        >
          Clear
        </button>

        <div data-testid="traces-count" class="ml-auto text-xs text-text-secondary">
          {traces().length} trace{traces().length !== 1 ? 's' : ''}
        </div>
      </form>

      {/* Table */}
      <div class="flex-1 overflow-auto">
        <Show when={error()}>
          <div class="px-6 py-8 text-center">
            <p class="text-error text-sm">{error()}</p>
            <button
              onClick={() => { setLoading(true); loadTraces(); }}
              class="mt-2 text-accent hover:text-accent-hover text-sm"
            >
              Retry
            </button>
          </div>
        </Show>

        <Show when={loading() && traces().length === 0}>
          <div class="px-6 py-4 space-y-2">
            <For each={[1, 2, 3, 4, 5]}>
              {() => <Skeleton class="h-10 w-full" />}
            </For>
          </div>
        </Show>

        <Show when={!loading() || traces().length > 0}>
          <table class="w-full">
            <thead class="sticky top-0 z-10">
              <tr class="bg-surface-2/90 backdrop-blur text-xs text-text-secondary uppercase tracking-wider">
                <th class="text-left px-7 py-3 font-medium">Trace ID</th>
                <th class="text-left px-5 py-3 font-medium">Operation</th>
                <th class="text-left px-5 py-3 font-medium">Services</th>
                <th class="text-right px-5 py-3 font-medium">Duration</th>
                <th class="text-right px-5 py-3 font-medium">Spans</th>
                <th class="text-center px-5 py-3 font-medium">Status</th>
                <th class="text-right px-7 py-3 font-medium">Time</th>
              </tr>
            </thead>
            <tbody>
              <Show when={!loading() && !error() && traces().length === 0}>
                <tr><td colspan="7" class="px-6 py-12 text-center text-text-secondary text-sm">No traces found. Waiting for telemetry data...</td></tr>
              </Show>
              <For each={traces()}>
                {(trace) => (
                  <tr
                    data-testid="trace-row"
                    class="border-b border-border/30 hover:bg-surface-2/50 cursor-pointer transition-colors animate-fade-in"
                    onClick={() => { window.location.hash = `/traces/${trace.trace_id}`; }}
                  >
                    <td class="px-7 py-3.5">
                      <span data-testid="trace-id" class="font-mono text-sm text-accent hover:text-accent-hover">
                        {truncateId(trace.trace_id)}
                      </span>
                    </td>
                    <td class="px-5 py-3.5 text-sm text-text-secondary max-w-[200px] truncate">
                      {trace.root_operation || '(unknown)'}
                    </td>
                    <td class="px-5 py-3.5">
                      <div class="flex flex-wrap gap-1">
                        <For each={trace.services}>
                          {(svc) => (
                            <Badge variant="default">{svc}</Badge>
                          )}
                        </For>
                      </div>
                    </td>
                    <td class="px-5 py-3.5 text-right">
                      <span class={`text-sm font-mono ${
                        trace.duration_ms > 1000 ? 'text-warning' : 'text-text-secondary'
                      }`}>
                        {formatDuration(trace.duration_ms)}
                      </span>
                    </td>
                    <td class="px-5 py-3.5 text-right text-sm text-text-secondary">
                      {trace.span_count}
                    </td>
                    <td class="px-5 py-3.5 text-center">
                      <Badge data-testid="trace-status-badge" variant={trace.has_error ? 'error' : 'success'}>
                        {trace.has_error ? 'Error' : 'Ok'}
                      </Badge>
                    </td>
                    <td class="px-7 py-3.5 text-right text-xs text-text-secondary font-mono">
                      {formatTime(trace.start_time)}
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

export default TracesView;
