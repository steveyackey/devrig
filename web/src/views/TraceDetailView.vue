<script setup lang="ts">
import { ref, computed, watch } from "vue";
import { useRoute, RouterLink } from "vue-router";
import {
  fetchTrace,
  fetchRelated,
  type TraceDetailResponse,
  type StoredSpan,
  type StoredSpanEvent,
  type RelatedResponse,
} from "@/api";
import { formatDuration, severityVariant } from "@/lib/format";

const route = useRoute();
const traceId = computed(() => String(route.params["id"] ?? ""));

interface SpanNode {
  span: StoredSpan;
  children: SpanNode[];
  depth: number;
}

/** A row in the waterfall: either a span or an event belonging to a span. */
type WaterfallItem =
  | { kind: "span"; node: SpanNode }
  | { kind: "event"; event: StoredSpanEvent; parentSpan: StoredSpan; depth: number };

/** Framework/internal events get visually dimmed vs. business events. */
function isFrameworkEvent(name: string): boolean {
  const lower = name.toLowerCase();
  return (
    lower.startsWith("executing ") ||
    lower.startsWith("preparing ") ||
    lower.startsWith("connecting") ||
    lower.includes("statement ") ||
    lower.includes(" with parameters") ||
    lower.includes(" with types")
  );
}

const traceData = ref<TraceDetailResponse | null>(null);
const related = ref<RelatedResponse | null>(null);
const loading = ref(true);
const error = ref<string | null>(null);
const selectedSpan = ref<StoredSpan | null>(null);
const expandedSpans = ref<Set<string>>(new Set());
const activeTab = ref<"spans" | "logs" | "metrics">("spans");

async function loadData() {
  try {
    loading.value = true;
    error.value = null;
    const [trace, rel] = await Promise.all([
      fetchTrace(traceId.value),
      fetchRelated(traceId.value),
    ]);
    traceData.value = trace;
    related.value = rel;
  } catch (err: unknown) {
    error.value = err instanceof Error ? err.message : "Failed to load trace";
  } finally {
    loading.value = false;
  }
}

watch(traceId, loadData, { immediate: true });

const spanTree = computed((): SpanNode[] => {
  const data = traceData.value;
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
      node.children.sort(
        (a, b) => new Date(a.span.start_time).getTime() - new Date(b.span.start_time).getTime(),
      );
      setDepths(node.children, depth + 1);
    }
  };

  roots.sort(
    (a, b) => new Date(a.span.start_time).getTime() - new Date(b.span.start_time).getTime(),
  );
  setDepths(roots, 0);

  return roots;
});

/** All span IDs that have events, for expand-all/collapse-all. */
const spanIdsWithEvents = computed((): Set<string> => {
  const ids = new Set<string>();
  for (const span of traceData.value?.spans ?? []) {
    if ((span.events?.length ?? 0) > 0) ids.add(span.span_id);
  }
  return ids;
});

function toggleSpanEvents(spanId: string) {
  const next = new Set(expandedSpans.value);
  if (next.has(spanId)) next.delete(spanId);
  else next.add(spanId);
  expandedSpans.value = next;
}

function expandAllEvents() {
  expandedSpans.value = new Set(spanIdsWithEvents.value);
}

function collapseAllEvents() {
  expandedSpans.value = new Set();
}

const allExpanded = computed(() => {
  const withEvents = spanIdsWithEvents.value;
  if (withEvents.size === 0) return false;
  const expanded = expandedSpans.value;
  for (const id of withEvents) {
    if (!expanded.has(id)) return false;
  }
  return true;
});

/** Flatten spans, interleaving events as rows under individually expanded spans. */
const waterfallItems = computed((): WaterfallItem[] => {
  const result: WaterfallItem[] = [];
  const expanded = expandedSpans.value;

  const flatten = (nodes: SpanNode[]) => {
    for (const node of nodes) {
      result.push({ kind: "span", node });
      const spanExpanded = expanded.has(node.span.span_id) && (node.span.events?.length ?? 0) > 0;

      if (spanExpanded) {
        // Collect child spans and events, sort by timestamp, interleave
        const childItems: { time: number; item: WaterfallItem }[] = [];

        for (const child of node.children) {
          childItems.push({
            time: new Date(child.span.start_time).getTime(),
            item: { kind: "span", node: child },
          });
        }

        for (const event of node.span.events ?? []) {
          childItems.push({
            time: new Date(event.timestamp).getTime(),
            item: { kind: "event", event, parentSpan: node.span, depth: node.depth + 2 },
          });
        }

        childItems.sort((a, b) => a.time - b.time);

        for (const { item } of childItems) {
          if (item.kind === "event") {
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

  flatten(spanTree.value);
  return result;
});

const timelineBounds = computed(() => {
  const data = traceData.value;
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

function eventPct(timestamp: string): number {
  const bounds = timelineBounds.value;
  return ((new Date(timestamp).getTime() - bounds.min) / (bounds.max - bounds.min)) * 100;
}

function spanLeftPct(span: StoredSpan): number {
  const bounds = timelineBounds.value;
  return ((new Date(span.start_time).getTime() - bounds.min) / (bounds.max - bounds.min)) * 100;
}

function spanWidthPct(span: StoredSpan): number {
  const bounds = timelineBounds.value;
  const start = new Date(span.start_time).getTime();
  const end = new Date(span.end_time).getTime();
  return Math.max(((end - start) / (bounds.max - bounds.min)) * 100, 0.5);
}

const totalEvents = computed(
  () => traceData.value?.spans.reduce((sum, s) => sum + (s.events?.length ?? 0), 0) ?? 0,
);

const traceHasError = computed(
  () => traceData.value?.spans.some((s) => s.status === "Error") ?? false,
);

const maxDuration = computed(() =>
  Math.max(...(traceData.value?.spans.map((s) => s.duration_ms) ?? []), 0),
);

function selectSpan(span: StoredSpan) {
  selectedSpan.value = selectedSpan.value?.span_id === span.span_id ? null : span;
}

function statusColor(status: string): string {
  switch (status) {
    case "Error":
      return "text-error";
    case "Ok":
      return "text-success";
    default:
      return "text-text-muted";
  }
}

function barGradient(status: string): string {
  switch (status) {
    case "Error":
      return "bg-gradient-to-r from-error/80 to-error/50";
    case "Ok":
      return "bg-gradient-to-r from-accent/60 to-accent/30";
    default:
      return "bg-gradient-to-r from-surface-3/80 to-surface-3/50";
  }
}

function badgeClass(variant: string): string {
  const base =
    "inline-flex items-center px-2 py-0.5 text-[9px] font-label uppercase tracking-wider transition-colors";
  switch (variant) {
    case "success":
      return `${base} text-success border border-success/30`;
    case "error":
      return `${base} text-error border border-error/30`;
    case "warning":
      return `${base} text-warning border border-warning/30`;
    case "info":
      return `${base} text-info border border-info/30`;
    default:
      return `${base} text-text-muted border border-border`;
  }
}

function tabClass(tab: string): string {
  const base =
    "px-4 py-2.5 font-display text-base tracking-[0.1em] uppercase border-b-2 -mb-px transition-colors";
  return activeTab.value === tab
    ? `${base} border-accent text-accent`
    : `${base} border-transparent text-text-muted hover:text-text-secondary`;
}

// Inlined ui component classes (ported from dashboard/src/components/ui)
const skeletonClass =
  "bg-gradient-to-r from-surface-2 via-surface-3 to-surface-2 bg-[length:400%_100%] animate-skeleton";
const thClass =
  "bg-surface-1 text-[10px] font-label text-text-muted uppercase tracking-[0.15em] px-5 py-3";
const rowClass = "border-b border-border hover:bg-accent/[0.03] transition-colors";

interface DetailRowItem {
  label: string;
  value: string;
  mono?: boolean;
}

const detailRowsTop = computed((): DetailRowItem[] => {
  const span = selectedSpan.value;
  if (!span) return [];
  return [
    { label: "Service", value: span.service_name },
    { label: "Operation", value: span.operation_name },
    { label: "Span ID", value: span.span_id, mono: true },
    { label: "Parent Span", value: span.parent_span_id ?? "(root)", mono: true },
    { label: "Kind", value: span.kind },
  ];
});

const detailRowsBottom = computed((): DetailRowItem[] => {
  const span = selectedSpan.value;
  if (!span) return [];
  return [
    { label: "Duration", value: formatDuration(span.duration_ms) },
    { label: "Start", value: new Date(span.start_time).toISOString(), mono: true },
    { label: "End", value: new Date(span.end_time).toISOString(), mono: true },
  ];
});

function formatLocalTime(timestamp: string): string {
  return new Date(timestamp).toLocaleTimeString();
}
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Header -->
    <div class="px-8 py-5 border-b-2 border-border flex items-center gap-4">
      <RouterLink
        to="/traces"
        class="text-text-muted hover:text-accent font-label text-[9px] uppercase tracking-[0.08em] flex items-center gap-1"
      >
        <span>&#x2190;</span> Back to Traces
      </RouterLink>
      <div class="h-4 border-l border-border" />
      <div>
        <h2
          class="font-display text-2xl text-accent tracking-[0.1em] uppercase flex items-center gap-2"
        >
          Trace Detail
          <span v-if="traceData" :class="badgeClass(traceHasError ? 'error' : 'success')">
            {{ traceHasError ? "Error" : "Ok" }}
          </span>
        </h2>
        <p class="text-xs text-text-secondary font-mono mt-0.5">{{ traceId }}</p>
      </div>

      <div v-if="traceData" class="ml-auto flex items-center gap-4 text-sm text-text-secondary">
        <span
          >{{ traceData.spans.length }} {{ traceData.spans.length !== 1 ? "spans" : "span" }}</span
        >
        <span v-if="totalEvents > 0"
          >{{ totalEvents }} {{ totalEvents !== 1 ? "events" : "event" }}</span
        >
        <span>{{ formatDuration(maxDuration) }}</span>
      </div>
    </div>

    <!-- Loading / Error states -->
    <div v-if="loading" class="flex-1 p-7 space-y-3">
      <div :class="[skeletonClass, 'h-8 w-48']" />
      <div v-for="n in 4" :key="n" :class="[skeletonClass, 'h-10 w-full']" />
    </div>

    <div v-if="error" class="flex-1 flex items-center justify-center">
      <div class="text-center">
        <p class="text-error text-sm">{{ error }}</p>
        <button class="mt-2 text-accent hover:text-accent-hover text-sm" @click="loadData">
          Retry
        </button>
      </div>
    </div>

    <!-- Main content -->
    <div v-if="!loading && !error && traceData" class="flex flex-1 overflow-hidden">
      <!-- Span waterfall -->
      <div class="flex-1 overflow-auto p-7">
        <div>
          <div class="flex items-center justify-between">
            <div role="tablist" class="flex border-b-2 border-border px-7">
              <button
                role="tab"
                :aria-selected="activeTab === 'spans'"
                :class="tabClass('spans')"
                @click="activeTab = 'spans'"
              >
                Spans ({{ traceData.spans.length }})
              </button>
              <button
                role="tab"
                :aria-selected="activeTab === 'logs'"
                :class="tabClass('logs')"
                @click="activeTab = 'logs'"
              >
                Logs ({{ related?.logs.length ?? 0 }})
              </button>
              <button
                role="tab"
                :aria-selected="activeTab === 'metrics'"
                :class="tabClass('metrics')"
                @click="activeTab = 'metrics'"
              >
                Metrics ({{ related?.metrics.length ?? 0 }})
              </button>
            </div>

            <!-- Expand/collapse all events toggle -->
            <button
              v-if="totalEvents > 0"
              class="text-[10px] text-text-muted hover:text-warning transition-colors uppercase tracking-[0.08em] select-none flex items-center gap-1.5"
              @click="allExpanded ? collapseAllEvents() : expandAllEvents()"
            >
              <span class="text-warning/60">&#x25C6;</span>
              {{ allExpanded ? "Collapse all events" : "Expand all events" }}
            </button>
          </div>

          <!-- Spans tab - Waterfall view -->
          <div v-if="activeTab === 'spans'" role="tabpanel" class="animate-fade-in">
            <div class="py-4">
              <div
                v-if="waterfallItems.length === 0"
                class="px-7 py-8 text-center text-text-secondary text-sm"
              >
                No spans found.
              </div>
              <template v-for="(item, idx) in waterfallItems" :key="idx">
                <!-- Event row — subtle annotation, subordinate to spans -->
                <div
                  v-if="item.kind === 'event'"
                  data-testid="waterfall-event-row"
                  class="flex items-center rounded px-2 transition-colors cursor-pointer"
                  :class="
                    isFrameworkEvent(item.event.name)
                      ? 'opacity-50 hover:opacity-80'
                      : 'opacity-80 hover:opacity-100'
                  "
                  style="padding-top: 1px; padding-bottom: 1px"
                  @click="selectedSpan = item.parentSpan"
                >
                  <!-- Label area — indented text only -->
                  <div
                    class="shrink-0 flex items-center gap-1.5 pr-3 overflow-hidden"
                    :style="{ width: '280px', paddingLeft: `${item.depth * 20}px` }"
                  >
                    <span
                      class="truncate"
                      :class="
                        isFrameworkEvent(item.event.name)
                          ? 'text-[9px] text-text-muted'
                          : 'text-[10px] text-warning'
                      "
                      :title="item.event.name"
                    >
                      {{ item.event.name }}
                    </span>
                  </div>

                  <!-- Timeline: single diamond marker at position -->
                  <div class="flex-1 relative h-3">
                    <div
                      class="absolute top-0 bottom-0 flex items-center"
                      :style="{ left: `${eventPct(item.event.timestamp)}%`, marginLeft: '-3px' }"
                    >
                      <div
                        class="rotate-45"
                        :class="
                          isFrameworkEvent(item.event.name)
                            ? 'w-1.5 h-1.5 bg-text-muted/60'
                            : 'w-[7px] h-[7px] bg-warning border border-warning/50'
                        "
                      />
                    </div>
                  </div>
                </div>

                <!-- Span row -->
                <div
                  v-else
                  data-testid="waterfall-row"
                  class="flex items-center hover:bg-surface-2/60 cursor-pointer rounded px-2 py-1 transition-colors animate-fade-in"
                  :class="
                    selectedSpan?.span_id === item.node.span.span_id
                      ? 'bg-surface-2 ring-1 ring-accent/30'
                      : ''
                  "
                  @click="selectSpan(item.node.span)"
                >
                  <!-- Label area -->
                  <div
                    class="shrink-0 flex items-center gap-1 pr-3 overflow-hidden"
                    :style="{ width: '280px', paddingLeft: `${item.node.depth * 20}px` }"
                  >
                    <span v-if="item.node.depth > 0" class="text-border text-xs select-none"
                      >&#x2514;</span
                    >
                    <span class="text-xs text-text-muted truncate">{{
                      item.node.span.service_name
                    }}</span>
                    <span class="text-border text-xs">/</span>
                    <span
                      class="text-xs truncate"
                      :class="
                        item.node.span.status === 'Error' ? 'text-error' : 'text-text-secondary'
                      "
                    >
                      {{ item.node.span.operation_name }}
                    </span>
                    <!-- Clickable event count badge — expands/collapses this span's events -->
                    <button
                      v-if="(item.node.span.events?.length ?? 0) > 0"
                      class="text-[9px] ml-1 shrink-0 transition-colors"
                      :class="
                        expandedSpans.has(item.node.span.span_id)
                          ? 'text-warning hover:text-warning/70'
                          : 'text-warning/50 hover:text-warning'
                      "
                      :title="`${expandedSpans.has(item.node.span.span_id) ? 'Collapse' : 'Expand'} ${item.node.span.events.length} event${item.node.span.events.length !== 1 ? 's' : ''}`"
                      @click.stop="toggleSpanEvents(item.node.span.span_id)"
                    >
                      <span
                        class="inline-block"
                        :style="{
                          transform: expandedSpans.has(item.node.span.span_id)
                            ? 'rotate(0deg)'
                            : 'rotate(-90deg)',
                          transition: 'transform 0.15s ease',
                        }"
                        >&#x25BE;</span
                      >&#x25C6;{{ item.node.span.events.length }}
                    </button>
                  </div>

                  <!-- Timeline bar area -->
                  <div
                    data-testid="waterfall-bar"
                    class="flex-1 relative h-6 bg-surface-2/30 rounded overflow-hidden"
                  >
                    <div
                      class="absolute top-1 bottom-1 rounded-sm"
                      :class="barGradient(item.node.span.status)"
                      :style="{
                        left: `${spanLeftPct(item.node.span)}%`,
                        width: `${spanWidthPct(item.node.span)}%`,
                        minWidth: '2px',
                      }"
                    />
                    <!-- Diamond markers on span bar — shown when this span's events are collapsed -->
                    <template
                      v-if="
                        !expandedSpans.has(item.node.span.span_id) &&
                        (item.node.span.events?.length ?? 0) > 0
                      "
                    >
                      <div
                        v-for="(event, ei) in item.node.span.events"
                        :key="ei"
                        class="absolute top-0 bottom-0 flex items-center"
                        :style="{ left: `${eventPct(event.timestamp)}%`, marginLeft: '-3px' }"
                        :title="`${event.name} @ ${formatLocalTime(event.timestamp)}`"
                      >
                        <div
                          class="rotate-45"
                          :class="
                            isFrameworkEvent(event.name)
                              ? 'w-1.5 h-1.5 bg-warning/40'
                              : 'w-[7px] h-[7px] bg-warning/80 border border-warning/50'
                          "
                        />
                      </div>
                    </template>
                    <span
                      class="absolute top-0.5 text-[10px] text-text-muted font-mono whitespace-nowrap"
                      :style="{
                        left: `${Math.min(spanLeftPct(item.node.span) + spanWidthPct(item.node.span) + 1, 85)}%`,
                      }"
                    >
                      {{ formatDuration(item.node.span.duration_ms) }}
                    </span>
                  </div>
                </div>
              </template>
            </div>
          </div>

          <!-- Logs tab -->
          <div v-if="activeTab === 'logs'" role="tabpanel" class="animate-fade-in">
            <div class="overflow-auto">
              <div
                v-if="related?.logs.length === 0"
                class="px-7 py-8 text-center text-text-secondary text-sm"
              >
                No related logs found for this trace.
              </div>
              <table v-if="(related?.logs.length ?? 0) > 0" class="w-full">
                <thead class="sticky top-0 z-10">
                  <tr :class="rowClass">
                    <th :class="[thClass, 'text-left']">Time</th>
                    <th :class="[thClass, 'text-left']">Severity</th>
                    <th :class="[thClass, 'text-left']">Service</th>
                    <th :class="[thClass, 'text-left']">Body</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="log in related?.logs ?? []" :key="log.record_id" :class="rowClass">
                    <td class="px-5 py-5 text-xs font-mono text-text-secondary whitespace-nowrap">
                      {{ formatLocalTime(log.timestamp) }}
                    </td>
                    <td class="px-5 py-5">
                      <span :class="badgeClass(severityVariant(log.severity))">{{
                        log.severity
                      }}</span>
                    </td>
                    <td class="px-5 py-5 text-xs text-text-secondary">{{ log.service_name }}</td>
                    <td class="px-5 py-5 text-sm text-text-secondary font-mono max-w-md truncate">
                      {{ log.body }}
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>

          <!-- Metrics tab -->
          <div v-if="activeTab === 'metrics'" role="tabpanel" class="animate-fade-in">
            <div class="overflow-auto">
              <div
                v-if="related?.metrics.length === 0"
                class="px-7 py-8 text-center text-text-secondary text-sm"
              >
                No related metrics found for this trace.
              </div>
              <table v-if="(related?.metrics.length ?? 0) > 0" class="w-full">
                <thead class="sticky top-0 z-10">
                  <tr :class="rowClass">
                    <th :class="[thClass, 'text-left']">Time</th>
                    <th :class="[thClass, 'text-left']">Service</th>
                    <th :class="[thClass, 'text-left']">Name</th>
                    <th :class="[thClass, 'text-left']">Type</th>
                    <th :class="[thClass, 'text-right']">Value</th>
                    <th :class="[thClass, 'text-left']">Unit</th>
                  </tr>
                </thead>
                <tbody>
                  <tr
                    v-for="metric in related?.metrics ?? []"
                    :key="metric.record_id"
                    :class="rowClass"
                  >
                    <td class="px-5 py-5 text-xs font-mono text-text-secondary whitespace-nowrap">
                      {{ formatLocalTime(metric.timestamp) }}
                    </td>
                    <td class="px-5 py-5 text-xs text-text-secondary">{{ metric.service_name }}</td>
                    <td class="px-5 py-5 text-sm text-text-secondary font-mono">
                      {{ metric.metric_name }}
                    </td>
                    <td class="px-5 py-5">
                      <span :class="badgeClass('default')">{{ metric.metric_type }}</span>
                    </td>
                    <td class="px-5 py-5 text-right text-sm font-mono text-text-secondary">
                      {{ metric.value.toFixed(2) }}
                    </td>
                    <td class="px-5 py-5 text-xs text-text-secondary">{{ metric.unit ?? "-" }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </div>

      <!-- Span detail panel -->
      <div
        v-if="selectedSpan"
        class="border-2 border-border bg-surface-1 w-96 border-l-2 overflow-auto shrink-0 max-h-screen"
      >
        <div class="px-6 py-5 border-b-2 border-border flex flex-row items-center justify-between">
          <h3 class="font-display text-lg text-accent tracking-[0.1em] uppercase">Span Details</h3>
          <button
            class="text-text-muted hover:text-text-primary text-sm"
            @click="selectedSpan = null"
          >
            &#x2715;
          </button>
        </div>

        <div class="p-6 space-y-4">
          <!-- Core info -->
          <div class="space-y-2">
            <div
              v-for="row in detailRowsTop"
              :key="row.label"
              class="flex items-center justify-between gap-2"
            >
              <span class="text-xs text-text-secondary shrink-0">{{ row.label }}</span>
              <span
                class="text-xs text-text-secondary truncate text-right"
                :class="row.mono ? 'font-mono' : ''"
                :title="row.value"
              >
                {{ row.value }}
              </span>
            </div>
            <div class="flex items-center justify-between">
              <span class="text-xs text-text-secondary">Status</span>
              <span class="text-xs font-medium" :class="statusColor(selectedSpan.status)">
                {{ selectedSpan.status
                }}{{ selectedSpan.status_message ? `: ${selectedSpan.status_message}` : "" }}
              </span>
            </div>
            <div
              v-for="row in detailRowsBottom"
              :key="row.label"
              class="flex items-center justify-between gap-2"
            >
              <span class="text-xs text-text-secondary shrink-0">{{ row.label }}</span>
              <span
                class="text-xs text-text-secondary truncate text-right"
                :class="row.mono ? 'font-mono' : ''"
                :title="row.value"
              >
                {{ row.value }}
              </span>
            </div>
          </div>

          <!-- Attributes -->
          <div v-if="selectedSpan.attributes.length > 0">
            <h4 class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em] mb-2">
              Attributes ({{ selectedSpan.attributes.length }})
            </h4>
            <div class="bg-surface-2/50 border border-border divide-y divide-border">
              <div
                v-for="[key, value] in selectedSpan.attributes"
                :key="key"
                class="px-3 py-2 flex gap-2"
              >
                <span class="text-xs text-text-muted shrink-0 font-mono">{{ key }}</span>
                <span class="text-xs text-text-secondary font-mono break-all ml-auto text-right">
                  {{ value }}
                </span>
              </div>
            </div>
          </div>

          <!-- Span Events -->
          <div v-if="(selectedSpan.events?.length ?? 0) > 0">
            <h4 class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em] mb-2">
              Events ({{ selectedSpan.events.length }})
            </h4>
            <div class="bg-surface-2/50 border border-border divide-y divide-border">
              <div v-for="(event, ei) in selectedSpan.events" :key="ei" class="px-3 py-2">
                <div class="flex justify-between items-center">
                  <span class="text-xs font-medium text-warning flex items-center gap-1.5">
                    <span class="w-2 h-2 bg-warning rotate-45 inline-block" />
                    {{ event.name }}
                  </span>
                  <span class="text-[10px] font-mono text-text-muted">
                    {{ formatLocalTime(event.timestamp) }}
                  </span>
                </div>
                <div v-if="event.attributes.length > 0" class="mt-1 space-y-0.5">
                  <div
                    v-for="[key, value] in event.attributes"
                    :key="key"
                    class="flex gap-2 text-[10px]"
                  >
                    <span class="text-text-muted font-mono">{{ key }}</span>
                    <span class="text-text-secondary font-mono break-all ml-auto text-right">{{
                      value
                    }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
