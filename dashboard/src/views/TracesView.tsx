import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { fetchTraces, fetchStatus, type TraceSummary, type TelemetryEvent } from '../api';

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

  // Initial load and auto-refresh
  createEffect(() => {
    loadTraces();
    loadServices();
    const interval = setInterval(loadTraces, 10000);
    onCleanup(() => clearInterval(interval));
  });

  // React to WebSocket trace updates
  createEffect(() => {
    const event = props.onEvent;
    if (event && event.type === 'TraceUpdate') {
      // Refresh traces when we get a new trace update
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
    <div class="flex flex-col h-full">
      {/* Header */}
      <div class="px-6 py-4 border-b border-zinc-700/50">
        <h2 class="text-lg font-semibold text-zinc-100">Traces</h2>
        <p class="text-sm text-zinc-500 mt-0.5">Distributed trace overview</p>
      </div>

      {/* Filter Bar */}
      <form onSubmit={handleSearch} class="px-6 py-3 border-b border-zinc-700/50 flex items-center gap-3 flex-wrap">
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

        <div class="flex items-center gap-2">
          <label class="text-xs text-zinc-500 uppercase tracking-wider">Status</label>
          <select
            value={filterStatus()}
            onChange={(e) => setFilterStatus(e.currentTarget.value)}
            class="bg-zinc-800 border border-zinc-700 rounded-md px-3 py-1.5 text-sm text-zinc-200 focus:outline-none focus:border-blue-500 min-w-[120px]"
          >
            <option value="">All</option>
            <option value="Ok">Ok</option>
            <option value="Error">Error</option>
          </select>
        </div>

        <div class="flex items-center gap-2">
          <label class="text-xs text-zinc-500 uppercase tracking-wider">Min Duration</label>
          <input
            type="number"
            placeholder="ms"
            value={filterMinDuration()}
            onInput={(e) => setFilterMinDuration(e.currentTarget.value)}
            class="bg-zinc-800 border border-zinc-700 rounded-md px-3 py-1.5 text-sm text-zinc-200 focus:outline-none focus:border-blue-500 w-24"
          />
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
            setFilterService('');
            setFilterStatus('');
            setFilterMinDuration('');
            setLoading(true);
            loadTraces();
          }}
          class="text-zinc-400 hover:text-zinc-200 text-sm px-3 py-1.5"
        >
          Clear
        </button>

        <div class="ml-auto text-xs text-zinc-600">
          {traces().length} trace{traces().length !== 1 ? 's' : ''}
        </div>
      </form>

      {/* Table */}
      <div class="flex-1 overflow-auto">
        <Show when={error()}>
          <div class="px-6 py-8 text-center">
            <p class="text-red-400 text-sm">{error()}</p>
            <button
              onClick={() => { setLoading(true); loadTraces(); }}
              class="mt-2 text-blue-400 hover:text-blue-300 text-sm"
            >
              Retry
            </button>
          </div>
        </Show>

        <Show when={loading() && traces().length === 0}>
          <div class="px-6 py-12 text-center text-zinc-500 text-sm">
            Loading traces...
          </div>
        </Show>

        <Show when={!loading() && !error() && traces().length === 0}>
          <div class="px-6 py-12 text-center text-zinc-500 text-sm">
            No traces found. Waiting for telemetry data...
          </div>
        </Show>

        <Show when={traces().length > 0}>
          <table class="w-full">
            <thead class="sticky top-0 z-10">
              <tr class="bg-zinc-800/90 backdrop-blur text-xs text-zinc-500 uppercase tracking-wider">
                <th class="text-left px-6 py-2.5 font-medium">Trace ID</th>
                <th class="text-left px-4 py-2.5 font-medium">Operation</th>
                <th class="text-left px-4 py-2.5 font-medium">Services</th>
                <th class="text-right px-4 py-2.5 font-medium">Duration</th>
                <th class="text-right px-4 py-2.5 font-medium">Spans</th>
                <th class="text-center px-4 py-2.5 font-medium">Status</th>
                <th class="text-right px-6 py-2.5 font-medium">Time</th>
              </tr>
            </thead>
            <tbody>
              <For each={traces()}>
                {(trace) => (
                  <tr
                    class="border-b border-zinc-800/50 hover:bg-zinc-800/50 cursor-pointer"
                    onClick={() => { window.location.hash = `/traces/${trace.trace_id}`; }}
                  >
                    <td class="px-6 py-3">
                      <span class="font-mono text-sm text-blue-400 hover:text-blue-300">
                        {truncateId(trace.trace_id)}
                      </span>
                    </td>
                    <td class="px-4 py-3 text-sm text-zinc-300 max-w-[200px] truncate">
                      {trace.root_operation || '(unknown)'}
                    </td>
                    <td class="px-4 py-3">
                      <div class="flex flex-wrap gap-1">
                        <For each={trace.services}>
                          {(svc) => (
                            <span class="inline-block bg-zinc-700/50 text-zinc-300 text-xs px-2 py-0.5 rounded">
                              {svc}
                            </span>
                          )}
                        </For>
                      </div>
                    </td>
                    <td class="px-4 py-3 text-right">
                      <span class={`text-sm font-mono ${
                        trace.duration_ms > 1000 ? 'text-yellow-400' : 'text-zinc-300'
                      }`}>
                        {formatDuration(trace.duration_ms)}
                      </span>
                    </td>
                    <td class="px-4 py-3 text-right text-sm text-zinc-400">
                      {trace.span_count}
                    </td>
                    <td class="px-4 py-3 text-center">
                      {trace.has_error ? (
                        <span class="inline-block bg-red-500/15 text-red-400 text-xs font-medium px-2 py-0.5 rounded-full border border-red-500/20">
                          Error
                        </span>
                      ) : (
                        <span class="inline-block bg-green-500/15 text-green-400 text-xs font-medium px-2 py-0.5 rounded-full border border-green-500/20">
                          Ok
                        </span>
                      )}
                    </td>
                    <td class="px-6 py-3 text-right text-xs text-zinc-500 font-mono">
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
