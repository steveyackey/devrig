import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { fetchLogs, fetchStatus, type StoredLog, type TelemetryEvent } from '../api';
import { Badge, Skeleton } from '../components/ui';

interface LogsViewProps {
  onEvent?: TelemetryEvent | null;
}

const LogsView: Component<LogsViewProps> = (props) => {
  const [logs, setLogs] = createSignal<StoredLog[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [services, setServices] = createSignal<string[]>([]);

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

  createEffect(() => {
    loadLogs();
    loadServices();
  });

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

  const severityVariant = (severity: string) => {
    switch (severity) {
      case 'Fatal': return 'fatal' as const;
      case 'Error': return 'error' as const;
      case 'Warn': return 'warning' as const;
      case 'Info': return 'info' as const;
      case 'Debug': return 'debug' as const;
      case 'Trace': return 'trace' as const;
      default: return 'default' as const;
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
    <div data-testid="logs-view" class="flex flex-col h-full">
      <div class="px-7 py-6 border-b border-border">
        <h2 class="text-xl font-semibold text-text-primary">Logs</h2>
        <p class="text-sm text-text-secondary mt-0.5">Application log records</p>
      </div>

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
          <label class="text-xs text-text-secondary uppercase tracking-wider">Severity</label>
          <select
            value={filterSeverity()}
            onChange={(e) => setFilterSeverity(e.currentTarget.value)}
            class="bg-surface-2 border border-border rounded-md px-3.5 py-2 text-sm text-text-primary focus:outline-none focus:border-accent min-w-[120px]"
          >
            <option value="">All</option>
            <For each={severities}>
              {(sev) => <option value={sev}>{sev}</option>}
            </For>
          </select>
        </div>

        <div class="flex items-center gap-2">
          <label class="text-xs text-text-secondary uppercase tracking-wider">Search</label>
          <input
            type="text"
            placeholder="Search log body..."
            value={filterSearch()}
            onInput={(e) => setFilterSearch(e.currentTarget.value)}
            class="bg-surface-2 border border-border rounded-md px-3.5 py-2 text-sm text-text-primary focus:outline-none focus:border-accent w-60"
          />
        </div>

        <button type="submit" class="bg-accent hover:bg-accent-hover text-white text-sm font-medium px-5 py-2 rounded-md transition-colors">
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
          class="text-text-secondary hover:text-text-primary text-sm px-3.5 py-2"
        >
          Clear
        </button>

        <div data-testid="logs-count" class="ml-auto text-xs text-text-secondary">
          {logs().length} log{logs().length !== 1 ? 's' : ''}
        </div>
      </form>

      <div class="flex-1 overflow-auto">
        <Show when={error()}>
          <div class="px-6 py-8 text-center">
            <p class="text-error text-sm">{error()}</p>
            <button onClick={() => { setLoading(true); loadLogs(); }} class="mt-2 text-accent hover:text-accent-hover text-sm">Retry</button>
          </div>
        </Show>

        <Show when={loading() && logs().length === 0}>
          <div class="px-6 py-4 space-y-2">
            <For each={[1, 2, 3, 4, 5]}>{() => <Skeleton class="h-8 w-full" />}</For>
          </div>
        </Show>

        <Show when={!loading() || logs().length > 0}>
          <table class="w-full">
            <thead class="sticky top-0 z-10">
              <tr class="bg-surface-2/90 backdrop-blur text-xs text-text-secondary uppercase tracking-wider">
                <th class="text-left px-5 py-3 font-medium w-32">Time</th>
                <th class="text-left px-4 py-3 font-medium w-20">Severity</th>
                <th class="text-left px-4 py-3 font-medium w-32">Service</th>
                <th class="text-left px-4 py-3 font-medium">Body</th>
                <th class="text-left px-5 py-3 font-medium w-32">Trace</th>
              </tr>
            </thead>
            <tbody>
              <Show when={!loading() && !error() && logs().length === 0}>
                <tr><td colspan="5" class="px-6 py-12 text-center text-text-secondary text-sm">No logs found. Adjust filters or wait for new data.</td></tr>
              </Show>
              <For each={logs()}>
                {(log) => (
                  <tr data-testid="log-row" class="border-b border-border/30 hover:bg-surface-2/40 group animate-fade-in">
                    <td class="px-5 py-3.5 align-top">
                      <span data-testid="log-timestamp" class="text-xs font-mono text-text-secondary whitespace-nowrap">
                        {formatTime(log.timestamp)}
                      </span>
                    </td>
                    <td class="px-4 py-3.5 align-top">
                      <Badge data-testid="log-severity-badge" variant={severityVariant(log.severity)}>
                        {log.severity}
                      </Badge>
                    </td>
                    <td class="px-4 py-3.5 text-xs text-text-secondary align-top truncate max-w-[130px]">
                      {log.service_name}
                    </td>
                    <td class="px-4 py-3.5 text-sm text-text-secondary font-mono align-top">
                      <div data-testid="log-body" class="whitespace-pre-wrap break-all max-h-24 overflow-hidden group-hover:max-h-none">
                        {log.body}
                      </div>
                    </td>
                    <td class="px-5 py-3.5 align-top">
                      <Show when={log.trace_id}>
                        <a
                          data-testid="log-trace-link"
                          href={`#/traces/${log.trace_id}`}
                          class="text-xs font-mono text-accent hover:text-accent-hover"
                          onClick={(e) => e.stopPropagation()}
                        >
                          {log.trace_id!.slice(0, 8)}...
                        </a>
                      </Show>
                      <Show when={!log.trace_id}>
                        <span class="text-xs text-text-secondary">-</span>
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
