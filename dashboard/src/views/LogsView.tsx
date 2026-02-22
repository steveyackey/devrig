import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { fetchLogs, fetchStatus, type StoredLog, type TelemetryEvent } from '../api';

interface LogsViewProps {
  onEvent?: TelemetryEvent | null;
}

const LogsView: Component<LogsViewProps> = (props) => {
  const [logs, setLogs] = createSignal<StoredLog[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [services, setServices] = createSignal<string[]>([]);

  // Filters
  const [filterService, setFilterService] = createSignal('');
  const [filterSeverity, setFilterSeverity] = createSignal('');
  const [filterSearch, setFilterSearch] = createSignal('');

  const loadLogs = async () => {
    try {
      setError(null);
      const data = await fetchLogs({
        service: filterService() || undefined,
        severity: filterSeverity() || undefined,
        search: filterSearch() || undefined,
        limit: 200,
      });
      setLogs(data);
    } catch (err: any) {
      setError(err.message || 'Failed to load logs');
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
    loadLogs();
    loadServices();
  });

  // React to WebSocket log events
  createEffect(() => {
    const event = props.onEvent;
    if (event && event.type === 'LogRecord') {
      loadLogs();
    }
  });

  const handleSearch = (e: Event) => {
    e.preventDefault();
    setLoading(true);
    loadLogs();
  };

  const severityColor = (severity: string): string => {
    switch (severity) {
      case 'Fatal': return 'bg-red-600 text-white';
      case 'Error': return 'bg-red-500/20 text-red-400 border border-red-500/30';
      case 'Warn': return 'bg-yellow-500/20 text-yellow-400 border border-yellow-500/30';
      case 'Info': return 'bg-blue-500/20 text-blue-400 border border-blue-500/30';
      case 'Debug': return 'bg-zinc-600/20 text-zinc-400 border border-zinc-600/30';
      case 'Trace': return 'bg-zinc-700/20 text-zinc-500 border border-zinc-700/30';
      default: return 'bg-zinc-700/20 text-zinc-400 border border-zinc-700/30';
    }
  };

  const formatTime = (iso: string): string => {
    try {
      const d = new Date(iso);
      return d.toLocaleTimeString(undefined, {
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        fractionalSecondDigits: 3,
      } as Intl.DateTimeFormatOptions);
    } catch {
      return iso;
    }
  };

  const severities = ['Trace', 'Debug', 'Info', 'Warn', 'Error', 'Fatal'];

  return (
    <div class="flex flex-col h-full">
      {/* Header */}
      <div class="px-6 py-4 border-b border-zinc-700/50">
        <h2 class="text-lg font-semibold text-zinc-100">Logs</h2>
        <p class="text-sm text-zinc-500 mt-0.5">Application log records</p>
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
          <label class="text-xs text-zinc-500 uppercase tracking-wider">Severity</label>
          <select
            value={filterSeverity()}
            onChange={(e) => setFilterSeverity(e.currentTarget.value)}
            class="bg-zinc-800 border border-zinc-700 rounded-md px-3 py-1.5 text-sm text-zinc-200 focus:outline-none focus:border-blue-500 min-w-[120px]"
          >
            <option value="">All</option>
            <For each={severities}>
              {(sev) => <option value={sev}>{sev}</option>}
            </For>
          </select>
        </div>

        <div class="flex items-center gap-2">
          <label class="text-xs text-zinc-500 uppercase tracking-wider">Search</label>
          <input
            type="text"
            placeholder="Search log body..."
            value={filterSearch()}
            onInput={(e) => setFilterSearch(e.currentTarget.value)}
            class="bg-zinc-800 border border-zinc-700 rounded-md px-3 py-1.5 text-sm text-zinc-200 focus:outline-none focus:border-blue-500 w-60"
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
            setFilterSeverity('');
            setFilterSearch('');
            setLoading(true);
            loadLogs();
          }}
          class="text-zinc-400 hover:text-zinc-200 text-sm px-3 py-1.5"
        >
          Clear
        </button>

        <div class="ml-auto text-xs text-zinc-600">
          {logs().length} log{logs().length !== 1 ? 's' : ''}
        </div>
      </form>

      {/* Log entries */}
      <div class="flex-1 overflow-auto">
        <Show when={error()}>
          <div class="px-6 py-8 text-center">
            <p class="text-red-400 text-sm">{error()}</p>
            <button
              onClick={() => { setLoading(true); loadLogs(); }}
              class="mt-2 text-blue-400 hover:text-blue-300 text-sm"
            >
              Retry
            </button>
          </div>
        </Show>

        <Show when={loading() && logs().length === 0}>
          <div class="px-6 py-12 text-center text-zinc-500 text-sm">
            Loading logs...
          </div>
        </Show>

        <Show when={!loading() && !error() && logs().length === 0}>
          <div class="px-6 py-12 text-center text-zinc-500 text-sm">
            No logs found. Adjust filters or wait for new data.
          </div>
        </Show>

        <Show when={logs().length > 0}>
          <table class="w-full">
            <thead class="sticky top-0 z-10">
              <tr class="bg-zinc-800/90 backdrop-blur text-xs text-zinc-500 uppercase tracking-wider">
                <th class="text-left px-4 py-2.5 font-medium w-32">Time</th>
                <th class="text-left px-3 py-2.5 font-medium w-20">Severity</th>
                <th class="text-left px-3 py-2.5 font-medium w-32">Service</th>
                <th class="text-left px-3 py-2.5 font-medium">Body</th>
                <th class="text-left px-4 py-2.5 font-medium w-32">Trace</th>
              </tr>
            </thead>
            <tbody>
              <For each={logs()}>
                {(log) => (
                  <tr class="border-b border-zinc-800/30 hover:bg-zinc-800/40 group">
                    <td class="px-4 py-2 text-xs font-mono text-zinc-500 whitespace-nowrap align-top">
                      {formatTime(log.timestamp)}
                    </td>
                    <td class="px-3 py-2 align-top">
                      <span class={`inline-block text-xs font-medium px-2 py-0.5 rounded ${severityColor(log.severity)}`}>
                        {log.severity}
                      </span>
                    </td>
                    <td class="px-3 py-2 text-xs text-zinc-400 align-top truncate max-w-[130px]">
                      {log.service_name}
                    </td>
                    <td class="px-3 py-2 text-sm text-zinc-300 font-mono align-top">
                      <div class="whitespace-pre-wrap break-all max-h-24 overflow-hidden group-hover:max-h-none">
                        {log.body}
                      </div>
                    </td>
                    <td class="px-4 py-2 align-top">
                      <Show when={log.trace_id}>
                        <a
                          href={`#/traces/${log.trace_id}`}
                          class="text-xs font-mono text-blue-400 hover:text-blue-300"
                          onClick={(e) => e.stopPropagation()}
                        >
                          {log.trace_id!.slice(0, 8)}...
                        </a>
                      </Show>
                      <Show when={!log.trace_id}>
                        <span class="text-xs text-zinc-600">-</span>
                      </Show>
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

export default LogsView;
