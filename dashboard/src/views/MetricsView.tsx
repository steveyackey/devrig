import { Component, createSignal, createEffect, onCleanup, For, Show, createMemo } from 'solid-js';
import {
  fetchMetrics,
  fetchMetricSeries,
  fetchStatus,
  type StoredMetric,
  type MetricSeries,
  type TelemetryEvent,
} from '../api';
import { Badge, Skeleton, Input, Select, Button, Table, TableHeader, TableRow, TableHead, TableCell } from '../components/ui';
import MetricChart, { Sparkline } from '../components/MetricChart';

interface MetricsViewProps {
  onEvent?: TelemetryEvent | null;
}

interface MetricCard {
  name: string;
  type: string;
  unit: string | null;
  latestValue: number;
  services: string[];
  sparklineData: [number[], number[]] | null;
}

const MetricsView: Component<MetricsViewProps> = (props) => {
  const [metrics, setMetrics] = createSignal<StoredMetric[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [services, setServices] = createSignal<string[]>([]);
  const [selectedMetric, setSelectedMetric] = createSignal<string | null>(null);
  const [chartSeries, setChartSeries] = createSignal<MetricSeries[]>([]);
  const [chartLoading, setChartLoading] = createSignal(false);

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

  // Build metric cards from raw data
  const metricCards = createMemo((): MetricCard[] => {
    const m = metrics();
    const grouped = new Map<string, StoredMetric[]>();
    for (const metric of m) {
      const existing = grouped.get(metric.metric_name) ?? [];
      existing.push(metric);
      grouped.set(metric.metric_name, existing);
    }

    return Array.from(grouped.entries()).map(([name, items]) => {
      // Sort by timestamp for sparkline
      const sorted = [...items].sort(
        (a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime(),
      );

      const svcs = [...new Set(items.map((i) => i.service_name))];
      const latest = sorted[sorted.length - 1];

      // Build sparkline data [timestamps_in_seconds, values]
      let sparklineData: [number[], number[]] | null = null;
      if (sorted.length >= 2) {
        const timestamps = sorted.map((s) => Math.floor(new Date(s.timestamp).getTime() / 1000));
        const values = sorted.map((s) => s.value);
        sparklineData = [timestamps, values];
      }

      return {
        name,
        type: latest?.metric_type ?? 'Gauge',
        unit: latest?.unit ?? null,
        latestValue: latest?.value ?? 0,
        services: svcs,
        sparklineData,
      };
    });
  });

  // Load chart data when a metric is selected
  const loadChartData = async (metricName: string) => {
    setChartLoading(true);
    try {
      const resp = await fetchMetricSeries(metricName, filterService() || undefined);
      setChartSeries(resp.series);
    } catch {
      setChartSeries([]);
    } finally {
      setChartLoading(false);
    }
  };

  const handleCardClick = (name: string) => {
    if (selectedMetric() === name) {
      setSelectedMetric(null);
      setChartSeries([]);
    } else {
      setSelectedMetric(name);
      loadChartData(name);
    }
  };

  // Build uPlot aligned data from chart series
  const chartData = createMemo((): [number[], ...number[][]] | null => {
    const series = chartSeries();
    if (series.length === 0) return null;

    // Collect all unique timestamps across all series
    const allTimes = new Set<number>();
    for (const s of series) {
      for (const p of s.points) {
        allTimes.add(Math.floor(p.t / 1000)); // convert ms to seconds
      }
    }

    const sortedTimes = [...allTimes].sort((a, b) => a - b);
    if (sortedTimes.length === 0) return null;

    // Build value arrays, null-filling gaps
    const result: [number[], ...number[][]] = [sortedTimes];
    for (const s of series) {
      const valueMap = new Map<number, number>();
      for (const p of s.points) {
        valueMap.set(Math.floor(p.t / 1000), p.v);
      }
      result.push(sortedTimes.map((t) => valueMap.get(t) ?? 0));
    }

    return result;
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
    if (Math.abs(value) >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
    if (Math.abs(value) >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
    if (Number.isInteger(value)) return value.toLocaleString();
    return value.toFixed(2);
  };

  const metricTypeVariant = (type: string) => {
    switch (type) {
      case 'Counter': return 'counter' as const;
      case 'Gauge': return 'gauge' as const;
      case 'Histogram': return 'histogram' as const;
      default: return 'default' as const;
    }
  };

  const chartTypeForMetric = (type: string): 'line' | 'bar' => {
    return type === 'Histogram' ? 'bar' : 'line';
  };

  return (
    <div data-testid="metrics-view" class="flex flex-col h-full">
      {/* Header */}
      <div class="px-6 py-4 border-b border-border">
        <h2 class="text-lg font-semibold text-text-primary">Metrics</h2>
        <p class="text-sm text-text-muted mt-0.5">Telemetry metric data points</p>
      </div>

      {/* Filter Bar */}
      <form onSubmit={handleSearch} class="px-6 py-3 border-b border-border flex items-center gap-4 flex-wrap">
        <div class="flex items-center gap-2">
          <label class="text-xs text-text-secondary uppercase tracking-wider">Metric Name</label>
          <Input
            type="text"
            placeholder="Filter by name..."
            value={filterName()}
            onInput={(e) => setFilterName(e.currentTarget.value)}
            class="w-48"
          />
        </div>

        <div class="flex items-center gap-2">
          <label class="text-xs text-text-secondary uppercase tracking-wider">Service</label>
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

        <Button type="submit">Search</Button>

        <button
          type="button"
          onClick={() => {
            setFilterName('');
            setFilterService('');
            setLoading(true);
            loadMetrics();
          }}
          class="text-text-secondary hover:text-text-primary text-sm px-3.5 py-2"
        >
          Clear
        </button>

        <div data-testid="metrics-count" class="ml-auto text-xs text-text-secondary">
          {metrics().length} metric{metrics().length !== 1 ? 's' : ''}
        </div>
      </form>

      <div class="flex-1 overflow-auto">
        <Show when={error()}>
          <div class="p-6 text-center">
            <p class="text-error text-sm">{error()}</p>
            <button onClick={() => { setLoading(true); loadMetrics(); }} class="mt-2 text-accent hover:text-accent-hover text-sm">Retry</button>
          </div>
        </Show>

        <Show when={loading() && metrics().length === 0}>
          <div class="p-6 space-y-4">
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-5">
              <For each={[1, 2, 3]}>{() => <Skeleton class="h-28 rounded-lg" />}</For>
            </div>
          </div>
        </Show>

        <Show when={!loading() || metrics().length > 0}>
          <div class="p-6 space-y-6 animate-fade-in">
            {/* Metric Cards Grid */}
            <Show when={metricCards().length > 0}>
              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-5">
                <For each={metricCards()}>
                  {(card) => (
                    <button
                      data-testid="metric-card"
                      onClick={() => handleCardClick(card.name)}
                      class={`text-left rounded-lg border p-6 transition-all hover:border-accent/40 hover:bg-surface-2/50 ${
                        selectedMetric() === card.name
                          ? 'border-accent/50 bg-accent/5 ring-1 ring-accent/20'
                          : 'border-border bg-surface-1'
                      }`}
                    >
                      <div class="flex items-start justify-between mb-2">
                        <span class="text-xs font-mono text-text-secondary truncate max-w-[70%]" title={card.name}>
                          {card.name}
                        </span>
                        <Badge variant={metricTypeVariant(card.type)} class="text-[10px] shrink-0 ml-2">
                          {card.type}
                        </Badge>
                      </div>
                      <div class="flex items-end justify-between">
                        <div>
                          <span class="text-2xl font-semibold font-mono text-text-primary">
                            {formatValue(card.latestValue)}
                          </span>
                          <Show when={card.unit}>
                            <span class="text-xs text-text-secondary ml-1">{card.unit}</span>
                          </Show>
                        </div>
                        <Show when={card.sparklineData}>
                          {(data) => (
                            <div class="ml-2">
                              <Sparkline data={data()} width={100} height={32} />
                            </div>
                          )}
                        </Show>
                      </div>
                      <div class="mt-2 flex flex-wrap gap-1">
                        <For each={card.services}>
                          {(svc) => (
                            <span class="text-[10px] text-text-secondary bg-surface-2 rounded px-1.5 py-0.5">
                              {svc}
                            </span>
                          )}
                        </For>
                      </div>
                    </button>
                  )}
                </For>
              </div>
            </Show>

            {/* Expanded Chart Panel */}
            <Show when={selectedMetric()}>
              <div class="rounded-lg border border-border bg-surface-1 p-6">
                <div class="flex items-center justify-between mb-4">
                  <div>
                    <h3 class="text-sm font-semibold text-text-primary font-mono">{selectedMetric()}</h3>
                    <p class="text-xs text-text-secondary mt-0.5">
                      {chartSeries().length} series
                    </p>
                  </div>
                  <button
                    onClick={() => { setSelectedMetric(null); setChartSeries([]); }}
                    class="text-xs text-text-secondary hover:text-text-primary px-2 py-1 rounded hover:bg-surface-2"
                  >
                    Close
                  </button>
                </div>
                <Show when={chartLoading()}>
                  <Skeleton class="h-[300px] w-full rounded" />
                </Show>
                <Show when={!chartLoading() && chartData()}>
                  {(data) => {
                    const type = chartSeries()[0]?.metric_type ?? 'Gauge';
                    return (
                      <MetricChart
                        data={data() as any}
                        width={900}
                        height={300}
                        seriesLabels={chartSeries().map((s) => s.service_name)}
                        chartType={chartTypeForMetric(type)}
                      />
                    );
                  }}
                </Show>
                <Show when={!chartLoading() && !chartData()}>
                  <div class="h-[300px] flex items-center justify-center text-text-secondary text-sm">
                    No time-series data available for this metric.
                  </div>
                </Show>
              </div>
            </Show>

            {/* Data Table */}
            <Show when={metrics().length > 0}>
              <div class="rounded-lg border border-border overflow-hidden">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead class="text-left">Time</TableHead>
                      <TableHead class="text-left">Service</TableHead>
                      <TableHead class="text-left">Metric Name</TableHead>
                      <TableHead class="text-left">Type</TableHead>
                      <TableHead class="text-right">Value</TableHead>
                      <TableHead class="text-left">Unit</TableHead>
                    </TableRow>
                  </TableHeader>
                  <tbody>
                    <Show when={!loading() && !error() && metrics().length === 0}>
                      <tr><td colspan="6" class="text-center text-text-muted text-sm">No metrics found. Adjust filters or wait for new data.</td></tr>
                    </Show>
                    <For each={metrics()}>
                      {(metric) => (
                        <TableRow data-testid="metric-row" class="animate-fade-in">
                          <TableCell class="text-xs font-mono text-text-secondary whitespace-nowrap">
                            {formatTime(metric.timestamp)}
                          </TableCell>
                          <TableCell class="text-sm text-text-secondary">
                            {metric.service_name}
                          </TableCell>
                          <TableCell>
                            <span data-testid="metric-name" class="text-sm text-text-secondary font-mono">{metric.metric_name}</span>
                          </TableCell>
                          <TableCell>
                            <Badge data-testid="metric-type-badge" variant={metricTypeVariant(metric.metric_type)}>
                              {metric.metric_type}
                            </Badge>
                          </TableCell>
                          <TableCell class="text-right">
                            <span data-testid="metric-value" class="text-sm font-mono text-text-primary">{formatValue(metric.value)}</span>
                          </TableCell>
                          <TableCell class="text-sm text-text-secondary">
                            {metric.unit ?? '-'}
                          </TableCell>
                        </TableRow>
                      )}
                    </For>
                  </tbody>
                </Table>
              </div>
            </Show>

            {/* Empty state */}
            <Show when={!loading() && !error() && metrics().length === 0}>
              <div class="text-center py-16">
                <p class="text-text-secondary text-sm">No metrics found. Adjust filters or wait for new data.</p>
              </div>
            </Show>
          </div>
        </Show>
      </div>
    </div>
  );
};

export default MetricsView;
