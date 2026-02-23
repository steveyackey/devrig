import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { fetchLogs, fetchStatus, type StoredLog, type TelemetryEvent } from '../api';
import { Badge, Skeleton, Input, Select, Button, Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '../components/ui';

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
  const [filterSource, setFilterSource] = createSignal('');

  const loadLogs = async () => {
    try {
      setError(null);
      const data = await fetchLogs({
        service: filterService() || undefined,
        severity: filterSeverity() || undefined,
        search: filterSearch() || undefined,
        source: filterSource() || undefined,
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

  const getLogSource = (log: StoredLog): string => {
    const attr = log.attributes.find(([k]) => k === 'log.source');
    return attr ? attr[1] : '';
  };

  const sourceLabel = (source: string): string => {
    switch (source) {
      case 'stdout': return 'stdout';
      case 'stderr': return 'stderr';
      case 'docker': return 'docker';
      case 'otlp': return 'sdk';
      default: return source || '-';
    }
  };

  return (
    <div data-testid="logs-view" class="flex flex-col h-full">
      <div class="px-8 py-6 border-b-2 border-border">
        <h2
          class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
          style={{ "text-shadow": "2px 2px 0 rgba(0,0,0,0.5)" }}
        >
          Logs
        </h2>
        <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">Application log records</p>
      </div>

      <form onSubmit={handleSearch} class="px-7 py-4 border-b-2 border-border flex items-center gap-4 flex-wrap">
        <div class="flex items-center gap-2">
          <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]">Service</label>
          <Select
            value={filterService()}
            onChange={(e) => setFilterService(e.currentTarget.value)}
            class="min-w-[140px]"
          >
            <option value="">All Services</option>
            <For each={services()}>
              {(svc) => <option value={svc}>{svc}</option>}
            </For>
          </Select>
        </div>

        <div class="flex items-center gap-2">
          <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]">Severity</label>
          <Select
            value={filterSeverity()}
            onChange={(e) => setFilterSeverity(e.currentTarget.value)}
            class="min-w-[120px]"
          >
            <option value="">All</option>
            <For each={severities}>
              {(sev) => <option value={sev}>{sev}</option>}
            </For>
          </Select>
        </div>

        <div class="flex items-center gap-2">
          <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]">Source</label>
          <Select
            value={filterSource()}
            onChange={(e) => setFilterSource(e.currentTarget.value)}
            class="min-w-[120px]"
          >
            <option value="">All</option>
            <option value="process">Process</option>
            <option value="docker">Docker</option>
            <option value="otlp">SDK</option>
          </Select>
        </div>

        <div class="flex items-center gap-2">
          <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]">Search</label>
          <Input
            type="text"
            placeholder="Search log body..."
            value={filterSearch()}
            onInput={(e) => setFilterSearch(e.currentTarget.value)}
            class="w-60"
          />
        </div>

        <Button type="submit">Search</Button>

        <button
          type="button"
          onClick={() => {
            setFilterService('');
            setFilterSeverity('');
            setFilterSearch('');
            setFilterSource('');
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

      <div class="flex-1 overflow-auto p-7">
        <Show when={error()}>
          <div class="py-8 text-center">
            <p class="text-error text-sm">{error()}</p>
            <button onClick={() => { setLoading(true); loadLogs(); }} class="mt-2 text-accent hover:text-accent-hover text-sm">Retry</button>
          </div>
        </Show>

        <Show when={loading() && logs().length === 0}>
          <div class="py-4 space-y-2">
            <For each={[1, 2, 3, 4, 5]}>{() => <Skeleton class="h-10 w-full" />}</For>
          </div>
        </Show>

        <Show when={!loading() || logs().length > 0}>
          <div class="border-2 border-border overflow-hidden">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead class="text-left w-32">Time</TableHead>
                <TableHead class="text-left w-20">Severity</TableHead>
                <TableHead class="text-left w-16">Source</TableHead>
                <TableHead class="text-left w-32">Service</TableHead>
                <TableHead class="text-left">Body</TableHead>
                <TableHead class="text-left w-32">Trace</TableHead>
              </TableRow>
            </TableHeader>
            <tbody>
              <Show when={!loading() && !error() && logs().length === 0}>
                <tr><td colspan="6" class="px-5 py-12 text-center text-text-secondary text-sm">No logs found. Adjust filters or wait for new data.</td></tr>
              </Show>
              <For each={logs()}>
                {(log) => (
                  <TableRow data-testid="log-row" class="group animate-fade-in">
                    <TableCell class="align-top">
                      <span data-testid="log-timestamp" class="text-xs font-mono text-text-secondary whitespace-nowrap">
                        {formatTime(log.timestamp)}
                      </span>
                    </TableCell>
                    <TableCell class="align-top">
                      <Badge data-testid="log-severity-badge" variant={severityVariant(log.severity)}>
                        {log.severity}
                      </Badge>
                    </TableCell>
                    <TableCell class="align-top">
                      <span data-testid="log-source" class="text-[10px] font-mono text-text-muted uppercase">
                        {sourceLabel(getLogSource(log))}
                      </span>
                    </TableCell>
                    <TableCell class="text-xs text-text-secondary align-top truncate max-w-[130px]">
                      {log.service_name}
                    </TableCell>
                    <TableCell class="text-sm text-text-secondary font-mono align-top">
                      <div data-testid="log-body" class="whitespace-pre-wrap break-all max-h-24 overflow-hidden group-hover:max-h-none">
                        {log.body}
                      </div>
                    </TableCell>
                    <TableCell class="align-top">
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
                    </TableCell>
                  </TableRow>
                )}
              </For>
            </tbody>
          </Table>
          </div>
        </Show>
      </div>
    </div>
  );
};

export default LogsView;
