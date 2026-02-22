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
    // Re-run when traceId changes
    const _id = props.traceId;
    loadData();
  });

  // Build the span tree
  const spanTree = createMemo((): SpanNode[] => {
    const data = traceData();
    if (!data || data.spans.length === 0) return [];

    const spans = [...data.spans];
    const byId = new Map<string, SpanNode>();
    const roots: SpanNode[] = [];

    // Create nodes
    for (const span of spans) {
      byId.set(span.span_id, { span, children: [], depth: 0 });
    }

    // Build tree
    for (const span of spans) {
      const node = byId.get(span.span_id)!;
      if (span.parent_span_id && byId.has(span.parent_span_id)) {
        const parent = byId.get(span.parent_span_id)!;
        parent.children.push(node);
      } else {
        roots.push(node);
      }
    }

    // Calculate depths and flatten
    const setDepths = (nodes: SpanNode[], depth: number) => {
      for (const node of nodes) {
        node.depth = depth;
        // Sort children by start time
        node.children.sort((a, b) =>
          new Date(a.span.start_time).getTime() - new Date(b.span.start_time).getTime()
        );
        setDepths(node.children, depth + 1);
      }
    };

    // Sort roots by start time
    roots.sort((a, b) =>
      new Date(a.span.start_time).getTime() - new Date(b.span.start_time).getTime()
    );
    setDepths(roots, 0);

    return roots;
  });

  // Flatten tree for rendering
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

  // Calculate timeline bounds
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
      case 'Error': return 'text-red-400';
      case 'Ok': return 'text-green-400';
      default: return 'text-zinc-400';
    }
  };

  const statusBgColor = (status: string): string => {
    switch (status) {
      case 'Error': return 'bg-red-500/80';
      case 'Ok': return 'bg-blue-500/80';
      default: return 'bg-zinc-500/80';
    }
  };

  const severityColor = (severity: string): string => {
    switch (severity) {
      case 'Fatal': return 'bg-red-600 text-white';
      case 'Error': return 'bg-red-500/20 text-red-400';
      case 'Warn': return 'bg-yellow-500/20 text-yellow-400';
      case 'Info': return 'bg-blue-500/20 text-blue-400';
      case 'Debug': return 'bg-zinc-600/20 text-zinc-400';
      case 'Trace': return 'bg-zinc-700/20 text-zinc-500';
      default: return 'bg-zinc-700/20 text-zinc-400';
    }
  };

  return (
    <div class="flex flex-col h-full">
      {/* Header */}
      <div class="px-6 py-4 border-b border-zinc-700/50 flex items-center gap-4">
        <a
          href="#/traces"
          class="text-zinc-400 hover:text-zinc-200 text-sm flex items-center gap-1"
        >
          <span>{'\u2190'}</span> Back to Traces
        </a>
        <div class="h-4 border-l border-zinc-700" />
        <div>
          <h2 class="text-lg font-semibold text-zinc-100 flex items-center gap-2">
            Trace Detail
            <Show when={traceData()}>
              {(data) => {
                const hasError = data().spans.some(s => s.status === 'Error');
                return hasError ? (
                  <span class="text-xs bg-red-500/15 text-red-400 px-2 py-0.5 rounded-full border border-red-500/20">
                    Error
                  </span>
                ) : (
                  <span class="text-xs bg-green-500/15 text-green-400 px-2 py-0.5 rounded-full border border-green-500/20">
                    Ok
                  </span>
                );
              }}
            </Show>
          </h2>
          <p class="text-xs text-zinc-500 font-mono mt-0.5">{props.traceId}</p>
        </div>

        <Show when={traceData()}>
          <div class="ml-auto flex items-center gap-4 text-sm text-zinc-400">
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
        <div class="flex-1 flex items-center justify-center text-zinc-500 text-sm">
          Loading trace...
        </div>
      </Show>

      <Show when={error()}>
        <div class="flex-1 flex items-center justify-center">
          <div class="text-center">
            <p class="text-red-400 text-sm">{error()}</p>
            <button
              onClick={loadData}
              class="mt-2 text-blue-400 hover:text-blue-300 text-sm"
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
            <div class="flex border-b border-zinc-700/50 px-6">
              <button
                onClick={() => setActiveTab('spans')}
                class={`px-4 py-2.5 text-sm font-medium border-b-2 -mb-px ${
                  activeTab() === 'spans'
                    ? 'border-blue-500 text-blue-400'
                    : 'border-transparent text-zinc-500 hover:text-zinc-300'
                }`}
              >
                Spans ({traceData()!.spans.length})
              </button>
              <button
                onClick={() => setActiveTab('logs')}
                class={`px-4 py-2.5 text-sm font-medium border-b-2 -mb-px ${
                  activeTab() === 'logs'
                    ? 'border-blue-500 text-blue-400'
                    : 'border-transparent text-zinc-500 hover:text-zinc-300'
                }`}
              >
                Logs ({related()?.logs.length ?? 0})
              </button>
              <button
                onClick={() => setActiveTab('metrics')}
                class={`px-4 py-2.5 text-sm font-medium border-b-2 -mb-px ${
                  activeTab() === 'metrics'
                    ? 'border-blue-500 text-blue-400'
                    : 'border-transparent text-zinc-500 hover:text-zinc-300'
                }`}
              >
                Metrics ({related()?.metrics.length ?? 0})
              </button>
            </div>

            {/* Spans tab - Waterfall view */}
            <Show when={activeTab() === 'spans'}>
              <div class="px-2 py-2">
                <For each={flattenedSpans()} fallback={
                  <div class="px-6 py-8 text-center text-zinc-500 text-sm">No spans found.</div>
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
                        class={`flex items-center hover:bg-zinc-800/60 cursor-pointer rounded px-2 py-1 ${
                          isSelected ? 'bg-zinc-800 ring-1 ring-blue-500/30' : ''
                        }`}
                        onClick={() => setSelectedSpan(isSelected ? null : node.span)}
                      >
                        {/* Label area */}
                        <div
                          class="shrink-0 flex items-center gap-1 pr-3 overflow-hidden"
                          style={{ width: '280px', "padding-left": `${node.depth * 20}px` }}
                        >
                          <Show when={node.depth > 0}>
                            <span class="text-zinc-700 text-xs select-none">{'\u2514'}</span>
                          </Show>
                          <span class="text-xs text-zinc-500 truncate">{node.span.service_name}</span>
                          <span class="text-zinc-700 text-xs">/</span>
                          <span class={`text-xs truncate ${
                            node.span.status === 'Error' ? 'text-red-400' : 'text-zinc-300'
                          }`}>
                            {node.span.operation_name}
                          </span>
                        </div>

                        {/* Timeline bar area */}
                        <div class="flex-1 relative h-6 bg-zinc-800/30 rounded overflow-hidden">
                          <div
                            class={`absolute top-1 bottom-1 rounded-sm ${statusBgColor(node.span.status)}`}
                            style={{
                              left: `${leftPct}%`,
                              width: `${widthPct}%`,
                              "min-width": '2px',
                            }}
                          />
                          <span
                            class="absolute top-0.5 text-[10px] text-zinc-400 font-mono whitespace-nowrap"
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
                  <div class="px-6 py-8 text-center text-zinc-500 text-sm">
                    No related logs found for this trace.
                  </div>
                </Show>
                <table class="w-full">
                  <Show when={(related()?.logs.length ?? 0) > 0}>
                    <thead>
                      <tr class="text-xs text-zinc-500 uppercase tracking-wider bg-zinc-800/50">
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
                        <tr class="border-b border-zinc-800/30 hover:bg-zinc-800/40">
                          <td class="px-4 py-2 text-xs font-mono text-zinc-500 whitespace-nowrap">
                            {new Date(log.timestamp).toLocaleTimeString()}
                          </td>
                          <td class="px-4 py-2">
                            <span class={`text-xs font-medium px-2 py-0.5 rounded ${severityColor(log.severity)}`}>
                              {log.severity}
                            </span>
                          </td>
                          <td class="px-4 py-2 text-xs text-zinc-400">{log.service_name}</td>
                          <td class="px-4 py-2 text-sm text-zinc-300 font-mono max-w-md truncate">
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
                  <div class="px-6 py-8 text-center text-zinc-500 text-sm">
                    No related metrics found for this trace.
                  </div>
                </Show>
                <table class="w-full">
                  <Show when={(related()?.metrics.length ?? 0) > 0}>
                    <thead>
                      <tr class="text-xs text-zinc-500 uppercase tracking-wider bg-zinc-800/50">
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
                        <tr class="border-b border-zinc-800/30 hover:bg-zinc-800/40">
                          <td class="px-4 py-2 text-xs font-mono text-zinc-500 whitespace-nowrap">
                            {new Date(metric.timestamp).toLocaleTimeString()}
                          </td>
                          <td class="px-4 py-2 text-xs text-zinc-400">{metric.service_name}</td>
                          <td class="px-4 py-2 text-sm text-zinc-300 font-mono">{metric.metric_name}</td>
                          <td class="px-4 py-2">
                            <span class="text-xs bg-zinc-700/50 text-zinc-400 px-2 py-0.5 rounded">
                              {metric.metric_type}
                            </span>
                          </td>
                          <td class="px-4 py-2 text-right text-sm font-mono text-zinc-300">
                            {metric.value.toFixed(2)}
                          </td>
                          <td class="px-4 py-2 text-xs text-zinc-500">{metric.unit ?? '-'}</td>
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
              <div class="w-96 border-l border-zinc-700/50 overflow-auto bg-zinc-900/50 shrink-0">
                <div class="px-4 py-3 border-b border-zinc-700/50 flex items-center justify-between">
                  <h3 class="text-sm font-semibold text-zinc-200">Span Details</h3>
                  <button
                    onClick={() => setSelectedSpan(null)}
                    class="text-zinc-500 hover:text-zinc-300 text-sm"
                  >
                    {'\u2715'}
                  </button>
                </div>

                <div class="p-4 space-y-4">
                  {/* Core info */}
                  <div class="space-y-2">
                    <DetailRow label="Service" value={span().service_name} />
                    <DetailRow label="Operation" value={span().operation_name} />
                    <DetailRow label="Span ID" value={span().span_id} mono />
                    <DetailRow label="Parent Span" value={span().parent_span_id ?? '(root)'} mono />
                    <DetailRow label="Kind" value={span().kind} />
                    <div class="flex items-center justify-between">
                      <span class="text-xs text-zinc-500">Status</span>
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
                      <h4 class="text-xs text-zinc-500 uppercase tracking-wider mb-2">
                        Attributes ({span().attributes.length})
                      </h4>
                      <div class="bg-zinc-800/50 rounded-lg border border-zinc-700/30 divide-y divide-zinc-700/30">
                        <For each={span().attributes}>
                          {([key, value]) => (
                            <div class="px-3 py-2 flex gap-2">
                              <span class="text-xs text-zinc-500 shrink-0 font-mono">{key}</span>
                              <span class="text-xs text-zinc-300 font-mono break-all ml-auto text-right">
                                {value}
                              </span>
                            </div>
                          )}
                        </For>
                      </div>
                    </div>
                  </Show>
                </div>
              </div>
            )}
          </Show>
        </div>
      </Show>
    </div>
  );
};

const DetailRow: Component<{ label: string; value: string; mono?: boolean }> = (props) => (
  <div class="flex items-center justify-between gap-2">
    <span class="text-xs text-zinc-500 shrink-0">{props.label}</span>
    <span
      class={`text-xs text-zinc-300 truncate text-right ${props.mono ? 'font-mono' : ''}`}
      title={props.value}
    >
      {props.value}
    </span>
  </div>
);

export default TraceDetail;
