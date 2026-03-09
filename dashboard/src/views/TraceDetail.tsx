import { Component, createSignal, createEffect, For, Show, createMemo } from 'solid-js';
import {
  fetchTrace,
  fetchRelated,
  type TraceDetailResponse,
  type StoredSpan,
  type StoredSpanEvent,
  type StoredLog,
  type StoredMetric,
  type RelatedResponse,
} from '../api';
import { formatDuration, severityVariant } from '../lib/format';
import {
  Badge,
  Skeleton,
  Card,
  CardHeader,
  CardContent,
  Tabs,
  TabsList,
  TabsTrigger,
  TabsContent,
  Table,
  TableHeader,
  TableRow,
  TableHead,
  TableCell,
} from '../components/ui';

interface TraceDetailProps {
  traceId: string;
}

interface SpanNode {
  span: StoredSpan;
  children: SpanNode[];
  depth: number;
}

/** A row in the waterfall: either a span or an event belonging to a span. */
type WaterfallItem =
  | { kind: 'span'; node: SpanNode }
  | { kind: 'event'; event: StoredSpanEvent; parentSpan: StoredSpan; depth: number };

/** Framework/internal events get visually dimmed vs. business events. */
const isFrameworkEvent = (name: string): boolean => {
  const lower = name.toLowerCase();
  return (
    lower.startsWith('executing ') ||
    lower.startsWith('preparing ') ||
    lower.startsWith('connecting') ||
    lower.includes('statement ') ||
    lower.includes(' with parameters') ||
    lower.includes(' with types')
  );
};

const TraceDetail: Component<TraceDetailProps> = (props) => {
  const [traceData, setTraceData] = createSignal<TraceDetailResponse | null>(null);
  const [related, setRelated] = createSignal<RelatedResponse | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [selectedSpan, setSelectedSpan] = createSignal<StoredSpan | null>(null);
  const [expandedSpans, setExpandedSpans] = createSignal<Set<string>>(new Set());

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

  /** All span IDs that have events, for expand-all/collapse-all. */
  const spanIdsWithEvents = createMemo((): Set<string> => {
    const ids = new Set<string>();
    for (const span of traceData()?.spans ?? []) {
      if ((span.events?.length ?? 0) > 0) ids.add(span.span_id);
    }
    return ids;
  });

  const toggleSpanEvents = (spanId: string) => {
    const next = new Set(expandedSpans());
    if (next.has(spanId)) next.delete(spanId);
    else next.add(spanId);
    setExpandedSpans(next);
  };

  const expandAllEvents = () => setExpandedSpans(new Set(spanIdsWithEvents()));
  const collapseAllEvents = () => setExpandedSpans(new Set());

  const allExpanded = createMemo(() => {
    const withEvents = spanIdsWithEvents();
    if (withEvents.size === 0) return false;
    const expanded = expandedSpans();
    for (const id of withEvents) {
      if (!expanded.has(id)) return false;
    }
    return true;
  });

  /** Flatten spans, interleaving events as rows under individually expanded spans. */
  const waterfallItems = createMemo((): WaterfallItem[] => {
    const result: WaterfallItem[] = [];
    const expanded = expandedSpans();

    const flatten = (nodes: SpanNode[]) => {
      for (const node of nodes) {
        result.push({ kind: 'span', node });
        const spanExpanded = expanded.has(node.span.span_id) && (node.span.events?.length ?? 0) > 0;

        if (spanExpanded) {
          // Collect child spans and events, sort by timestamp, interleave
          const childItems: { time: number; item: WaterfallItem }[] = [];

          for (const child of node.children) {
            childItems.push({
              time: new Date(child.span.start_time).getTime(),
              item: { kind: 'span', node: child },
            });
          }

          for (const event of node.span.events ?? []) {
            childItems.push({
              time: new Date(event.timestamp).getTime(),
              item: { kind: 'event', event, parentSpan: node.span, depth: node.depth + 2 },
            });
          }

          childItems.sort((a, b) => a.time - b.time);

          for (const { item } of childItems) {
            if (item.kind === 'event') {
              result.push(item);
            } else {
              flatten([item.node]);
            }
          }
        } else {
          flatten(node.children);
        }
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

  const totalEvents = createMemo(() =>
    traceData()?.spans.reduce((sum, s) => sum + (s.events?.length ?? 0), 0) ?? 0
  );

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
      case 'Ok': return 'bg-gradient-to-r from-accent/60 to-accent/30';
      default: return 'bg-gradient-to-r from-surface-3/80 to-surface-3/50';
    }
  };

  return (
    <div class="flex flex-col h-full">
      {/* Header */}
      <div class="px-8 py-5 border-b-2 border-border flex items-center gap-4">
        <a
          href="#/traces"
          class="text-text-muted hover:text-accent font-label text-[9px] uppercase tracking-[0.08em] flex items-center gap-1"
        >
          <span>{'\u2190'}</span> Back to Traces
        </a>
        <div class="h-4 border-l border-border" />
        <div>
          <h2 class="font-display text-2xl text-accent tracking-[0.1em] uppercase flex items-center gap-2">
            Trace Detail
            <Show when={traceData()}>
              {(data) => {
                const hasError = data().spans.some(s => s.status === 'Error');
                return <Badge variant={hasError ? 'error' : 'success'}>{hasError ? 'Error' : 'Ok'}</Badge>;
              }}
            </Show>
          </h2>
          <p class="text-xs text-text-secondary font-mono mt-0.5">{props.traceId}</p>
        </div>

        <Show when={traceData()}>
          {(data) => (
            <div class="ml-auto flex items-center gap-4 text-sm text-text-secondary">
              <span>{data().spans.length} span{data().spans.length !== 1 ? 's' : ''}</span>
              <Show when={totalEvents() > 0}>
                <span>{totalEvents()} event{totalEvents() !== 1 ? 's' : ''}</span>
              </Show>
              <span>
                {formatDuration(
                  Math.max(...data().spans.map(s => s.duration_ms), 0)
                )}
              </span>
            </div>
          )}
        </Show>
      </div>

      {/* Loading / Error states */}
      <Show when={loading()}>
        <div class="flex-1 p-7 space-y-3">
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
          <div class="flex-1 overflow-auto p-7">
            <Tabs defaultValue="spans">
              <div class="flex items-center justify-between">
                <TabsList>
                  <TabsTrigger value="spans">
                    Spans ({traceData()!.spans.length})
                  </TabsTrigger>
                  <TabsTrigger value="logs">
                    Logs ({related()?.logs.length ?? 0})
                  </TabsTrigger>
                  <TabsTrigger value="metrics">
                    Metrics ({related()?.metrics.length ?? 0})
                  </TabsTrigger>
                </TabsList>

                {/* Expand/collapse all events toggle */}
                <Show when={totalEvents() > 0}>
                  <button
                    onClick={() => allExpanded() ? collapseAllEvents() : expandAllEvents()}
                    class="text-[10px] text-text-muted hover:text-warning transition-colors uppercase tracking-[0.08em] select-none flex items-center gap-1.5"
                  >
                    <span class="text-warning/60">{'\u25C6'}</span>
                    {allExpanded() ? 'Collapse all events' : 'Expand all events'}
                  </button>
                </Show>
              </div>

              {/* Spans tab - Waterfall view */}
              <TabsContent value="spans">
                <div class="py-4">
                  <For each={waterfallItems()} fallback={
                    <div class="px-7 py-8 text-center text-text-secondary text-sm">No spans found.</div>
                  }>
                    {(item) => {
                      const bounds = timelineBounds();
                      const totalDuration = bounds.max - bounds.min;

                      if (item.kind === 'event') {
                        // Event row — subtle annotation, subordinate to spans
                        const eventTime = new Date(item.event.timestamp).getTime();
                        const eventPct = ((eventTime - bounds.min) / totalDuration) * 100;
                        const framework = isFrameworkEvent(item.event.name);

                        return (
                          <div
                            data-testid="waterfall-event-row"
                            class={`flex items-center rounded px-2 transition-colors cursor-pointer ${
                              framework
                                ? 'opacity-50 hover:opacity-80'
                                : 'opacity-80 hover:opacity-100'
                            }`}
                            style={{ "padding-top": '1px', "padding-bottom": '1px' }}
                            onClick={() => setSelectedSpan(item.parentSpan)}
                          >
                            {/* Label area — indented text only */}
                            <div
                              class="shrink-0 flex items-center gap-1.5 pr-3 overflow-hidden"
                              style={{ width: '280px', "padding-left": `${item.depth * 20}px` }}
                            >
                              <span class={`truncate ${
                                framework ? 'text-[9px] text-text-muted' : 'text-[10px] text-warning'
                              }`} title={item.event.name}>
                                {item.event.name}
                              </span>
                            </div>

                            {/* Timeline: single diamond marker at position */}
                            <div class="flex-1 relative h-3">
                              <div
                                class="absolute top-0 bottom-0 flex items-center"
                                style={{ left: `${eventPct}%`, "margin-left": '-3px' }}
                              >
                                <div class={`rotate-45 ${
                                  framework
                                    ? 'w-1.5 h-1.5 bg-text-muted/60'
                                    : 'w-[7px] h-[7px] bg-warning border border-warning/50'
                                }`} />
                              </div>
                            </div>
                          </div>
                        );
                      }

                      // Span row
                      const node = item.node;
                      const spanStart = new Date(node.span.start_time).getTime();
                      const spanEnd = new Date(node.span.end_time).getTime();
                      const leftPct = ((spanStart - bounds.min) / totalDuration) * 100;
                      const widthPct = Math.max(((spanEnd - spanStart) / totalDuration) * 100, 0.5);
                      const isSelected = selectedSpan()?.span_id === node.span.span_id;
                      const eventCount = node.span.events?.length ?? 0;
                      const isExpanded = expandedSpans().has(node.span.span_id);

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
                            {/* Clickable event count badge — expands/collapses this span's events */}
                            <Show when={eventCount > 0}>
                              <button
                                class={`text-[9px] ml-1 shrink-0 transition-colors ${
                                  isExpanded
                                    ? 'text-warning hover:text-warning/70'
                                    : 'text-warning/50 hover:text-warning'
                                }`}
                                title={`${isExpanded ? 'Collapse' : 'Expand'} ${eventCount} event${eventCount !== 1 ? 's' : ''}`}
                                onClick={(e) => { e.stopPropagation(); toggleSpanEvents(node.span.span_id); }}
                              >
                                <span class="inline-block" style={{ transform: isExpanded ? 'rotate(0deg)' : 'rotate(-90deg)', transition: 'transform 0.15s ease' }}>{'\u25BE'}</span>
                                {'\u25C6'}{eventCount}
                              </button>
                            </Show>
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
                            {/* Diamond markers on span bar — shown when this span's events are collapsed */}
                            <Show when={!isExpanded && eventCount > 0}>
                              <For each={node.span.events}>
                                {(event) => {
                                  const eventTime = new Date(event.timestamp).getTime();
                                  const eventPct = ((eventTime - bounds.min) / totalDuration) * 100;
                                  const framework = isFrameworkEvent(event.name);
                                  return (
                                    <div
                                      class="absolute top-0 bottom-0 flex items-center"
                                      style={{ left: `${eventPct}%`, "margin-left": '-3px' }}
                                      title={`${event.name} @ ${new Date(event.timestamp).toLocaleTimeString()}`}
                                    >
                                      <div class={`rotate-45 ${
                                        framework
                                          ? 'w-1.5 h-1.5 bg-warning/40'
                                          : 'w-[7px] h-[7px] bg-warning/80 border border-warning/50'
                                      }`} />
                                    </div>
                                  );
                                }}
                              </For>
                            </Show>
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
              </TabsContent>

              {/* Logs tab */}
              <TabsContent value="logs">
                <div class="overflow-auto">
                  <Show when={related()?.logs.length === 0}>
                    <div class="px-7 py-8 text-center text-text-secondary text-sm">
                      No related logs found for this trace.
                    </div>
                  </Show>
                  <Show when={(related()?.logs.length ?? 0) > 0}>
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead class="text-left">Time</TableHead>
                          <TableHead class="text-left">Severity</TableHead>
                          <TableHead class="text-left">Service</TableHead>
                          <TableHead class="text-left">Body</TableHead>
                        </TableRow>
                      </TableHeader>
                      <tbody>
                        <For each={related()?.logs ?? []}>
                          {(log) => (
                            <TableRow>
                              <TableCell class="text-xs font-mono text-text-secondary whitespace-nowrap">
                                {new Date(log.timestamp).toLocaleTimeString()}
                              </TableCell>
                              <TableCell>
                                <Badge variant={severityVariant(log.severity)}>
                                  {log.severity}
                                </Badge>
                              </TableCell>
                              <TableCell class="text-xs text-text-secondary">{log.service_name}</TableCell>
                              <TableCell class="text-sm text-text-secondary font-mono max-w-md truncate">
                                {log.body}
                              </TableCell>
                            </TableRow>
                          )}
                        </For>
                      </tbody>
                    </Table>
                  </Show>
                </div>
              </TabsContent>

              {/* Metrics tab */}
              <TabsContent value="metrics">
                <div class="overflow-auto">
                  <Show when={related()?.metrics.length === 0}>
                    <div class="px-7 py-8 text-center text-text-secondary text-sm">
                      No related metrics found for this trace.
                    </div>
                  </Show>
                  <Show when={(related()?.metrics.length ?? 0) > 0}>
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead class="text-left">Time</TableHead>
                          <TableHead class="text-left">Service</TableHead>
                          <TableHead class="text-left">Name</TableHead>
                          <TableHead class="text-left">Type</TableHead>
                          <TableHead class="text-right">Value</TableHead>
                          <TableHead class="text-left">Unit</TableHead>
                        </TableRow>
                      </TableHeader>
                      <tbody>
                        <For each={related()?.metrics ?? []}>
                          {(metric) => (
                            <TableRow>
                              <TableCell class="text-xs font-mono text-text-secondary whitespace-nowrap">
                                {new Date(metric.timestamp).toLocaleTimeString()}
                              </TableCell>
                              <TableCell class="text-xs text-text-secondary">{metric.service_name}</TableCell>
                              <TableCell class="text-sm text-text-secondary font-mono">{metric.metric_name}</TableCell>
                              <TableCell>
                                <Badge variant="default">{metric.metric_type}</Badge>
                              </TableCell>
                              <TableCell class="text-right text-sm font-mono text-text-secondary">
                                {metric.value.toFixed(2)}
                              </TableCell>
                              <TableCell class="text-xs text-text-secondary">{metric.unit ?? '-'}</TableCell>
                            </TableRow>
                          )}
                        </For>
                      </tbody>
                    </Table>
                  </Show>
                </div>
              </TabsContent>
            </Tabs>
          </div>

          {/* Span detail panel */}
          <Show when={selectedSpan()}>
            {(span) => (
              <Card class="w-96 border-l-2 border-border overflow-auto shrink-0 max-h-screen">
                <CardHeader class="flex flex-row items-center justify-between">
                  <h3 class="font-display text-lg text-accent tracking-[0.1em] uppercase">Span Details</h3>
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
                      <span class="text-xs text-text-secondary">Status</span>
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
                      <h4 class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em] mb-2">
                        Attributes ({span().attributes.length})
                      </h4>
                      <div class="bg-surface-2/50 border border-border divide-y divide-border">
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

                  {/* Span Events */}
                  <Show when={(span().events?.length ?? 0) > 0}>
                    <div>
                      <h4 class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em] mb-2">
                        Events ({span().events.length})
                      </h4>
                      <div class="bg-surface-2/50 border border-border divide-y divide-border">
                        <For each={span().events}>
                          {(event) => (
                            <div class="px-3 py-2">
                              <div class="flex justify-between items-center">
                                <span class="text-xs font-medium text-warning flex items-center gap-1.5">
                                  <span class="w-2 h-2 bg-warning rotate-45 inline-block" />
                                  {event.name}
                                </span>
                                <span class="text-[10px] font-mono text-text-muted">
                                  {new Date(event.timestamp).toLocaleTimeString()}
                                </span>
                              </div>
                              <Show when={event.attributes.length > 0}>
                                <div class="mt-1 space-y-0.5">
                                  <For each={event.attributes}>
                                    {([key, value]) => (
                                      <div class="flex gap-2 text-[10px]">
                                        <span class="text-text-muted font-mono">{key}</span>
                                        <span class="text-text-secondary font-mono break-all ml-auto text-right">{value}</span>
                                      </div>
                                    )}
                                  </For>
                                </div>
                              </Show>
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
    <span class="text-xs text-text-secondary shrink-0">{props.label}</span>
    <span
      class={`text-xs text-text-secondary truncate text-right ${props.mono ? 'font-mono' : ''}`}
      title={props.value}
    >
      {props.value}
    </span>
  </div>
);

export default TraceDetail;
