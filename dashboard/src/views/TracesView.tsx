import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { fetchTraces, fetchStatus, type TraceSummary, type TelemetryEvent } from '../api';
import { Badge, Skeleton, Input, Select, Button, Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '../components/ui';
import { formatDuration, formatTime } from '../lib/format';

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

  const truncateId = (id: string): string => {
    if (id.length <= 16) return id;
    return id.slice(0, 8) + '...' + id.slice(-4);
  };

  return (
    <div data-testid="traces-view" class="flex flex-col h-full">
      {/* Header */}
      <div class="px-8 py-6 border-b-2 border-border">
        <h2
          class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
          style={{ "text-shadow": "2px 2px 0 rgba(0,0,0,0.5)" }}
        >
          Traces
        </h2>
        <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">Distributed trace overview</p>
      </div>

      {/* Filter Bar */}
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
          <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]">Status</label>
          <Select
            value={filterStatus()}
            onChange={(e) => setFilterStatus(e.currentTarget.value)}
            class="min-w-[120px]"
          >
            <option value="">All</option>
            <option value="Ok">Ok</option>
            <option value="Error">Error</option>
          </Select>
        </div>

        <div class="flex items-center gap-2">
          <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]">Min Duration</label>
          <Input
            type="number"
            placeholder="ms"
            value={filterMinDuration()}
            onInput={(e) => setFilterMinDuration(e.currentTarget.value)}
            class="w-24"
          />
        </div>

        <Button type="submit">Search</Button>

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
      <div class="flex-1 overflow-auto p-7">
        <Show when={error()}>
          <div class="py-8 text-center">
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
          <div class="py-4 space-y-2">
            <For each={[1, 2, 3, 4, 5]}>
              {() => <Skeleton class="h-12 w-full" />}
            </For>
          </div>
        </Show>

        <Show when={!loading() || traces().length > 0}>
          <div class="border-2 border-border overflow-hidden">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead class="text-left">Trace ID</TableHead>
                <TableHead class="text-left">Operation</TableHead>
                <TableHead class="text-left">Services</TableHead>
                <TableHead class="text-right">Duration</TableHead>
                <TableHead class="text-right">Spans</TableHead>
                <TableHead class="text-center">Status</TableHead>
                <TableHead class="text-right">Time</TableHead>
              </TableRow>
            </TableHeader>
            <tbody>
              <Show when={!loading() && !error() && traces().length === 0}>
                <tr><td colspan="7" class="px-5 py-12 text-center text-text-secondary text-sm">No traces found. Waiting for telemetry data...</td></tr>
              </Show>
              <For each={traces()}>
                {(trace) => (
                  <TableRow
                    data-testid="trace-row"
                    class="cursor-pointer animate-fade-in"
                    onClick={() => { window.location.hash = `/traces/${trace.trace_id}`; }}
                  >
                    <TableCell>
                      <span data-testid="trace-id" class="font-mono text-sm text-accent hover:text-accent-hover">
                        {truncateId(trace.trace_id)}
                      </span>
                    </TableCell>
                    <TableCell class="text-sm text-text-secondary max-w-[200px] truncate">
                      {trace.root_operation || '(unknown)'}
                    </TableCell>
                    <TableCell>
                      <div class="flex flex-wrap gap-1">
                        <For each={trace.services}>
                          {(svc) => (
                            <Badge variant="default">{svc}</Badge>
                          )}
                        </For>
                      </div>
                    </TableCell>
                    <TableCell class="text-right">
                      <span class={`text-sm font-mono ${
                        trace.duration_ms > 1000 ? 'text-warning' : 'text-text-secondary'
                      }`}>
                        {formatDuration(trace.duration_ms)}
                      </span>
                    </TableCell>
                    <TableCell class="text-right text-sm text-text-secondary">
                      {trace.span_count}
                    </TableCell>
                    <TableCell class="text-center">
                      <Badge data-testid="trace-status-badge" variant={trace.has_error ? 'error' : 'success'}>
                        {trace.has_error ? 'Error' : 'Ok'}
                      </Badge>
                    </TableCell>
                    <TableCell class="text-right text-xs text-text-secondary font-mono">
                      {formatTime(trace.start_time)}
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

export default TracesView;
