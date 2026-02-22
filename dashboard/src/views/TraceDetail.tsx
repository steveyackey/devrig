import { Component, createSignal, createEffect, onCleanup, For, Show, createMemo } from 'solid-js';
import {
  fetchTrace,
  fetchRelated,
  type TraceDetailResponse,
  type StoredSpan,
  type StoredLog,
  type StoredMetric,
  type RelatedResponse,
} from '../api';
import { Badge, Skeleton, Card, CardHeader, CardContent } from '../components/ui';

interface TraceDetailProps {
  traceId: string;
}

interface SpanNode {
  span: StoredSpan;
  children: SpanNode[];
  depth: number;
}

const TraceDetail: Component<TraceDetailProps> = (props) => {
  const [traceData, setTraceData] = createSignal<TraceDetailResponse | null>(null);
  const [related, setRelated] = createSignal<RelatedResponse | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [selectedSpan, setSelectedSpan] = createSignal<StoredSpan | null>(null);
  const [activeTab, setActiveTab] = createSignal<'spans' | 'logs' | 'metrics'>('spans');

  const loadData = async () => {
    try {
      setLoading(true);
      setError(null);
      const [trace, rel] = await Promise.all([
        fetchTrace(props.traceId),
        fetchRelated(props.traceId),
      ]);
      setTraceData(trace);
      setRelated(rel);
    } catch (err: any) {
      setError(err.message || 'Failed to load trace');
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    const _id = props.traceId;
    loadData();
  });

  const spanTree = createMemo((): SpanNode[] => {
    const data = traceData();
    if (!data || data.spans.length === 0) return [];

    const spans = [...data.spans];
    const byId = new Map<string, SpanNode>();
    const roots: SpanNode[] = [];

    for (const span of spans) {
      byId.set(span.span_id, { span, children: [], depth: 0 });
    }

    for (const span of spans) {
      const node = byId.get(span.span_id)!;
      if (span.parent_span_id && byId.has(span.parent_span_id)) {
        const parent = byId.get(span.parent_span_id)!;
        parent.children.push(node);
      } else {
        roots.push(node);
      }
    }

    const setDepths = (nodes: SpanNode[], depth: number) => {
      for (const node of nodes) {
        node.depth = depth;
        node.children.sort((a, b) =>
          new Date(a.span.start_time).getTime() - new Date(b.span.start_time).getTime()
        );
        setDepths(node.children, depth + 1);
      }
    };

    roots.sort((a, b) =>
      new Date(a.span.start_time).getTime() - new Date(b.span.start_time).getTime()
    );
    setDepths(roots, 0);

    return roots;
  });

  const flattenedSpans = createMemo((): SpanNode[] => {
    const result: SpanNode[] = [];
    const flatten = (nodes: SpanNode[]) => {
      for (const node of nodes) {
        result.push(node);
        flatten(node.children);
      }
    };
    flatten(spanTree());
    return result;
  });

  const timelineBounds = createMemo(() => {
    const data = traceData();
    if (!data || data.spans.length === 0) return { min: 0, max: 1 };

    let min = Infinity;
    let max = -Infinity;

    for (const span of data.spans) {
      const start = new Date(span.start_time).getTime();
      const end = new Date(span.end_time).getTime();
      if (start < min) min = start;
      if (end > max) max = end;
    }

    if (max <= min) max = min + 1;
    return { min, max };
  });

  const formatDuration = (ms: number): string => {
    if (ms < 1) return '<1ms';
    if (ms < 1000) return `${ms.toFixed(1)}ms`;
    return `${(ms / 1000).toFixed(2)}s`;
  };

  const truncateId = (id: string): string => {
    if (id.length <= 12) return id;
    return id.slice(0, 8) + '...';
  };

  const statusColor = (status: string): string => {
    switch (status) {
      case 'Error': return 'text-error';
      case 'Ok': return 'text-success';
      default: return 'text-text-muted';
    }
  };

  const barGradient = (status: string): string => {
    switch (status) {
      case 'Error': return 'bg-gradient-to-r from-error/80 to-error/50';
      case 'Ok': return 'bg-gradient-to-r from-accent/80 to-accent/50';
      default: return 'bg-gradient-to-r from-surface-3/80 to-surface-3/50';
    }
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

  return (
    <div class="flex flex-col h-full">
      {/* Header */}
      <div class="px-6 py-4 border-b border-border flex items-center gap-4">
        <a
          href="#/traces"
          class="text-text-muted hover:text-text-primary text-sm flex items-center gap-1"
        >
          <span>{'\u2190'}</span> Back to Traces
        </a>
        <div class="h-4 border-l border-border" />
        <div>
          <h2 class="text-lg font-semibold text-text-primary flex items-center gap-2">
            Trace Detail
            <Show when={traceData()}>
              {(data) => {
                const hasError = data().spans.some(s => s.status === 'Error');
                return <Badge variant={hasError ? 'error' : 'success'}>{hasError ? 'Error' : 'Ok'}</Badge>;
              }}
            </Show>
          </h2>
          <p class="text-xs text-text-muted font-mono mt-0.5">{props.traceId}</p>
        </div>

        <Show when={traceData()}>
          <div class="ml-auto flex items-center gap-4 text-sm text-text-muted">
            <span>{traceData()!.spans.length} span{traceData()!.spans.length !== 1 ? 's' : ''}</span>
            <span>
              {formatDuration(
                Math.max(...traceData()!.spans.map(s => s.duration_ms), 0)
              )}
            </span>
          </div>
        </Show>
      </div>

      {/* Loading / Error states */}
      <Show when={loading()}>
        <div class="flex-1 p-6 space-y-3">
          <Skeleton class="h-8 w-48" />
          <For each={[1, 2, 3, 4]}>
            {() => <Skeleton class="h-10 w-full" />}
          </For>
        </div>
      </Show>

      <Show when={error()}>
        <div class="flex-1 flex items-center justify-center">
          <div class="text-center">
            <p class="text-error text-sm">{error()}</p>
            <button
              onClick={loadData}
              class="mt-2 text-accent hover:text-accent-hover text-sm"
            >
              Retry
            </button>
          </div>
        </div>
      </Show>

      {/* Main content */}
      <Show when={!loading() && !error() && traceData()}>
        <div class="flex flex-1 overflow-hidden">
          {/* Span waterfall */}
          <div class="flex-1 overflow-auto">
            {/* Tabs */}
            <div class="flex border-b border-border px-6">
              <button
                onClick={() => setActiveTab('spans')}
                class={`px-4 py-2.5 text-sm font-medium border-b-2 -mb-px transition-colors ${
                  activeTab() === 'spans'
                    ? 'border-accent text-accent'
                    : 'border-transparent text-text-muted hover:text-text-secondary'
                }`}
              >
                Spans ({traceData()!.spans.length})
              </button>
              <button
                onClick={() => setActiveTab('logs')}
                class={`px-4 py-2.5 text-sm font-medium border-b-2 -mb-px transition-colors ${
                  activeTab() === 'logs'
                    ? 'border-accent text-accent'
                    : 'border-transparent text-text-muted hover:text-text-secondary'
                }`}
              >
                Logs ({related()?.logs.length ?? 0})
              </button>
              <button
                onClick={() => setActiveTab('metrics')}
                class={`px-4 py-2.5 text-sm font-medium border-b-2 -mb-px transition-colors ${
                  activeTab() === 'metrics'
                    ? 'border-accent text-accent'
                    : 'border-transparent text-text-muted hover:text-text-secondary'
                }`}
              >
                Metrics ({related()?.metrics.length ?? 0})
              </button>
            </div>

            {/* Spans tab - Waterfall view */}
            <Show when={activeTab() === 'spans'}>
              <div class="px-2 py-2">
                <For each={flattenedSpans()} fallback={
                  <div class="px-6 py-8 text-center text-text-muted text-sm">No spans found.</div>
                }>
                  {(node) => {
                    const bounds = timelineBounds();
                    const spanStart = new Date(node.span.start_time).getTime();
                    const spanEnd = new Date(node.span.end_time).getTime();
                    const totalDuration = bounds.max - bounds.min;
                    const leftPct = ((spanStart - bounds.min) / totalDuration) * 100;
                    const widthPct = Math.max(((spanEnd - spanStart) / totalDuration) * 100, 0.5);
                    const isSelected = selectedSpan()?.span_id === node.span.span_id;

                    return (
                      <div
                        data-testid="waterfall-row"
                        class={`flex items-center hover:bg-surface-2/60 cursor-pointer rounded px-2 py-1 transition-colors animate-fade-in ${
                          isSelected ? 'bg-surface-2 ring-1 ring-accent/30' : ''
                        }`}
                        onClick={() => setSelectedSpan(isSelected ? null : node.span)}
                      >
                        {/* Label area */}
                        <div
                          class="shrink-0 flex items-center gap-1 pr-3 overflow-hidden"
                          style={{ width: '280px', "padding-left": `${node.depth * 20}px` }}
                        >
                          <Show when={node.depth > 0}>
                            <span class="text-border text-xs select-none">{'\u2514'}</span>
                          </Show>
                          <span class="text-xs text-text-muted truncate">{node.span.service_name}</span>
                          <span class="text-border text-xs">/</span>
                          <span class={`text-xs truncate ${
                            node.span.status === 'Error' ? 'text-error' : 'text-text-secondary'
                          }`}>
                            {node.span.operation_name}
                          </span>
                        </div>

                        {/* Timeline bar area */}
                        <div data-testid="waterfall-bar" class="flex-1 relative h-6 bg-surface-2/30 rounded overflow-hidden">
                          <div
                            class={`absolute top-1 bottom-1 rounded-sm ${barGradient(node.span.status)}`}
                            style={{
                              left: `${leftPct}%`,
                              width: `${widthPct}%`,
                              "min-width": '2px',
                            }}
                          />
                          <span
                            class="absolute top-0.5 text-[10px] text-text-muted font-mono whitespace-nowrap"
                            style={{ left: `${Math.min(leftPct + widthPct + 1, 85)}%` }}
                          >
                            {formatDuration(node.span.duration_ms)}
                          </span>
                        </div>
                      </div>
                    );
                  }}
                </For>
              </div>
            </Show>

            {/* Logs tab */}
            <Show when={activeTab() === 'logs'}>
              <div class="overflow-auto">
                <Show when={related()?.logs.length === 0}>
                  <div class="px-6 py-8 text-center text-text-muted text-sm">
                    No related logs found for this trace.
                  </div>
                </Show>
                <table class="w-full">
                  <Show when={(related()?.logs.length ?? 0) > 0}>
                    <thead>
                      <tr class="text-xs text-text-muted uppercase tracking-wider bg-surface-2/50">
                        <th class="text-left px-4 py-2 font-medium">Time</th>
                        <th class="text-left px-4 py-2 font-medium">Severity</th>
                        <th class="text-left px-4 py-2 font-medium">Service</th>
                        <th class="text-left px-4 py-2 font-medium">Body</th>
                      </tr>
                    </thead>
                  </Show>
                  <tbody>
                    <For each={related()?.logs ?? []}>
                      {(log) => (
                        <tr class="border-b border-border/30 hover:bg-surface-2/40">
                          <td class="px-4 py-2 text-xs font-mono text-text-muted whitespace-nowrap">
                            {new Date(log.timestamp).toLocaleTimeString()}
                          </td>
                          <td class="px-4 py-2">
                            <Badge variant={severityVariant(log.severity)}>
                              {log.severity}
                            </Badge>
                          </td>
                          <td class="px-4 py-2 text-xs text-text-muted">{log.service_name}</td>
                          <td class="px-4 py-2 text-sm text-text-secondary font-mono max-w-md truncate">
                            {log.body}
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>

            {/* Metrics tab */}
            <Show when={activeTab() === 'metrics'}>
              <div class="overflow-auto">
                <Show when={related()?.metrics.length === 0}>
                  <div class="px-6 py-8 text-center text-text-muted text-sm">
                    No related metrics found for this trace.
                  </div>
                </Show>
                <table class="w-full">
                  <Show when={(related()?.metrics.length ?? 0) > 0}>
                    <thead>
                      <tr class="text-xs text-text-muted uppercase tracking-wider bg-surface-2/50">
                        <th class="text-left px-4 py-2 font-medium">Time</th>
                        <th class="text-left px-4 py-2 font-medium">Service</th>
                        <th class="text-left px-4 py-2 font-medium">Name</th>
                        <th class="text-left px-4 py-2 font-medium">Type</th>
                        <th class="text-right px-4 py-2 font-medium">Value</th>
                        <th class="text-left px-4 py-2 font-medium">Unit</th>
                      </tr>
                    </thead>
                  </Show>
                  <tbody>
                    <For each={related()?.metrics ?? []}>
                      {(metric) => (
                        <tr class="border-b border-border/30 hover:bg-surface-2/40">
                          <td class="px-4 py-2 text-xs font-mono text-text-muted whitespace-nowrap">
                            {new Date(metric.timestamp).toLocaleTimeString()}
                          </td>
                          <td class="px-4 py-2 text-xs text-text-muted">{metric.service_name}</td>
                          <td class="px-4 py-2 text-sm text-text-secondary font-mono">{metric.metric_name}</td>
                          <td class="px-4 py-2">
                            <Badge variant="default">{metric.metric_type}</Badge>
                          </td>
                          <td class="px-4 py-2 text-right text-sm font-mono text-text-secondary">
                            {metric.value.toFixed(2)}
                          </td>
                          <td class="px-4 py-2 text-xs text-text-muted">{metric.unit ?? '-'}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>
          </div>

          {/* Span detail panel */}
          <Show when={selectedSpan()}>
            {(span) => (
              <Card class="w-96 border-l border-border overflow-auto shrink-0 rounded-none">
                <CardHeader class="flex flex-row items-center justify-between">
                  <h3 class="text-sm font-semibold text-text-primary">Span Details</h3>
                  <button
                    onClick={() => setSelectedSpan(null)}
                    class="text-text-muted hover:text-text-primary text-sm"
                  >
                    {'\u2715'}
                  </button>
                </CardHeader>

                <CardContent class="space-y-4">
                  {/* Core info */}
                  <div class="space-y-2">
                    <DetailRow label="Service" value={span().service_name} />
                    <DetailRow label="Operation" value={span().operation_name} />
                    <DetailRow label="Span ID" value={span().span_id} mono />
                    <DetailRow label="Parent Span" value={span().parent_span_id ?? '(root)'} mono />
                    <DetailRow label="Kind" value={span().kind} />
                    <div class="flex items-center justify-between">
                      <span class="text-xs text-text-muted">Status</span>
                      <span class={`text-xs font-medium ${statusColor(span().status)}`}>
                        {span().status}
                        {span().status_message ? `: ${span().status_message}` : ''}
                      </span>
                    </div>
                    <DetailRow label="Duration" value={formatDuration(span().duration_ms)} />
                    <DetailRow
                      label="Start"
                      value={new Date(span().start_time).toISOString()}
                      mono
                    />
                    <DetailRow
                      label="End"
                      value={new Date(span().end_time).toISOString()}
                      mono
                    />
                  </div>

                  {/* Attributes */}
                  <Show when={span().attributes.length > 0}>
                    <div>
                      <h4 class="text-xs text-text-muted uppercase tracking-wider mb-2">
                        Attributes ({span().attributes.length})
                      </h4>
                      <div class="bg-surface-2/50 rounded-lg border border-border divide-y divide-border">
                        <For each={span().attributes}>
                          {([key, value]) => (
                            <div class="px-3 py-2 flex gap-2">
                              <span class="text-xs text-text-muted shrink-0 font-mono">{key}</span>
                              <span class="text-xs text-text-secondary font-mono break-all ml-auto text-right">
                                {value}
                              </span>
                            </div>
                          )}
                        </For>
                      </div>
                    </div>
                  </Show>
                </CardContent>
              </Card>
            )}
          </Show>
        </div>
      </Show>
    </div>
  );
};

const DetailRow: Component<{ label: string; value: string; mono?: boolean }> = (props) => (
  <div class="flex items-center justify-between gap-2">
    <span class="text-xs text-text-muted shrink-0">{props.label}</span>
    <span
      class={`text-xs text-text-secondary truncate text-right ${props.mono ? 'font-mono' : ''}`}
      title={props.value}
    >
      {props.value}
    </span>
  </div>
);

export default TraceDetail;
