<script setup lang="ts">
import { ref, inject, watch, onMounted, onUnmounted } from "vue";
import { useRouter } from "vue-router";
import type { Ref } from "vue";
import { fetchTraces, fetchStatus, type TraceSummary, type TelemetryEvent } from "@/api";
import { formatDuration, formatTime } from "@/lib/format";

const router = useRouter();
const latestEvent = inject<Ref<TelemetryEvent | null>>(
  "latestEvent",
  ref<TelemetryEvent | null>(null),
);

const traces = ref<TraceSummary[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const services = ref<string[]>([]);

// Streaming
const streaming = ref(true);

// Filters
const filterService = ref("");
const filterStatus = ref("");
const filterMinDuration = ref("");
const filterSearch = ref("");

async function loadTraces() {
  try {
    error.value = null;
    const minDur = parseInt(filterMinDuration.value, 10);
    traces.value = await fetchTraces({
      service: filterService.value || undefined,
      status: filterStatus.value || undefined,
      min_duration_ms: Number.isNaN(minDur) ? undefined : minDur,
      search: filterSearch.value || undefined,
      limit: 100,
    });
  } catch (err: unknown) {
    error.value = err instanceof Error ? err.message : "Failed to load traces";
  } finally {
    loading.value = false;
  }
}

async function loadServices() {
  try {
    const status = await fetchStatus();
    services.value = status.services;
  } catch {
    // non-critical
  }
}

let interval: ReturnType<typeof setInterval> | null = null;

function startPolling() {
  if (interval) {
    clearInterval(interval);
    interval = null;
  }
  loadTraces();
  loadServices();
  if (streaming.value) {
    interval = setInterval(loadTraces, 10000);
  }
}

onMounted(startPolling);
watch(streaming, startPolling);
onUnmounted(() => {
  if (interval) clearInterval(interval);
});

watch(latestEvent, (event) => {
  if (event && event.type === "TraceUpdate" && streaming.value) {
    loadTraces();
  }
});

function handleSearch() {
  loading.value = true;
  loadTraces();
}

function handleClear() {
  filterService.value = "";
  filterStatus.value = "";
  filterMinDuration.value = "";
  filterSearch.value = "";
  loading.value = true;
  loadTraces();
}

function handleRetry() {
  loading.value = true;
  loadTraces();
}

function truncateId(id: string): string {
  if (id.length <= 16) return id;
  return id.slice(0, 8) + "..." + id.slice(-4);
}

function openTrace(traceId: string) {
  router.push(`/traces/${traceId}`);
}

type BadgeVariant = "default" | "success" | "error" | "warning";

function badgeClass(variant: BadgeVariant): string {
  const base =
    "inline-flex items-center px-2 py-0.5 text-[9px] font-label uppercase tracking-wider transition-colors";
  switch (variant) {
    case "success":
      return `${base} text-success border border-success/30`;
    case "error":
      return `${base} text-error border border-error/30`;
    case "warning":
      return `${base} text-warning border border-warning/30`;
    default:
      return `${base} text-text-muted border border-border`;
  }
}

function httpStatusVariant(code: number): BadgeVariant {
  return code >= 500 ? "error" : code >= 400 ? "warning" : code >= 300 ? "warning" : "success";
}

// Inlined ui component classes (ported from dashboard/src/components/ui)
const selectClass =
  "bg-surface-1 border-2 border-border px-3.5 py-2 text-sm text-text-primary focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent/30";
const inputClass = `${selectClass} placeholder:text-text-muted`;
const buttonClass =
  "inline-flex items-center justify-center gap-2 text-sm font-display tracking-[0.1em] uppercase transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50 disabled:pointer-events-none disabled:opacity-50 border-2 border-accent bg-transparent text-accent hover:bg-accent hover:text-surface-0 h-9 px-4 py-2";
const skeletonClass =
  "bg-gradient-to-r from-surface-2 via-surface-3 to-surface-2 bg-[length:400%_100%] animate-skeleton";
const thClass =
  "bg-surface-1 text-[10px] font-label text-text-muted uppercase tracking-[0.15em] px-5 py-3";
const rowClass = "border-b border-border hover:bg-accent/[0.03] transition-colors";
</script>

<template>
  <div data-testid="traces-view" class="flex flex-col h-full">
    <!-- Header -->
    <div class="px-8 py-6 border-b-2 border-border">
      <h2
        class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
        style="text-shadow: 2px 2px 0 rgba(0, 0, 0, 0.5)"
      >
        Traces
      </h2>
      <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">
        Distributed trace overview
      </p>
    </div>

    <!-- Filter Bar -->
    <form
      class="px-7 py-4 border-b-2 border-border flex items-center gap-4 flex-wrap"
      @submit.prevent="handleSearch"
    >
      <div class="flex items-center gap-2">
        <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]"
          >Service</label
        >
        <select v-model="filterService" :class="[selectClass, 'min-w-[140px]']">
          <option value="">All Services</option>
          <option v-for="svc in services" :key="svc" :value="svc">{{ svc }}</option>
        </select>
      </div>

      <div class="flex items-center gap-2">
        <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]"
          >Status</label
        >
        <select v-model="filterStatus" :class="[selectClass, 'min-w-[120px]']">
          <option value="">All</option>
          <option value="Ok">Ok</option>
          <option value="Error">Error</option>
        </select>
      </div>

      <div class="flex items-center gap-2">
        <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]"
          >Min Duration</label
        >
        <input
          v-model="filterMinDuration"
          type="number"
          placeholder="ms"
          :class="[inputClass, 'w-24']"
        />
      </div>

      <div class="flex items-center gap-2">
        <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]"
          >Operation</label
        >
        <input
          v-model="filterSearch"
          type="text"
          placeholder="Search operations..."
          :class="[inputClass, 'w-48']"
        />
      </div>

      <button type="submit" :class="buttonClass">Search</button>

      <button
        type="button"
        class="text-text-secondary hover:text-text-primary text-sm px-3.5 py-2"
        @click="handleClear"
      >
        Clear
      </button>

      <button
        type="button"
        class="ml-auto flex items-center gap-1.5 text-xs px-3 py-1.5 rounded border border-border hover:border-border-hover transition-colors"
        @click="streaming = !streaming"
      >
        <span
          class="inline-block w-2 h-2 rounded-full"
          :class="streaming ? 'bg-success animate-pulse-live' : 'bg-surface-3'"
        />
        {{ streaming ? "Live" : "Paused" }}
      </button>

      <div data-testid="traces-count" class="text-xs text-text-secondary">
        {{ traces.length }} {{ traces.length !== 1 ? "traces" : "trace" }}
      </div>
    </form>

    <!-- Table -->
    <div class="flex-1 overflow-auto p-7">
      <div v-if="error" class="py-8 text-center">
        <p class="text-error text-sm">{{ error }}</p>
        <button class="mt-2 text-accent hover:text-accent-hover text-sm" @click="handleRetry">
          Retry
        </button>
      </div>

      <div v-if="loading && traces.length === 0" class="py-4 space-y-2">
        <div v-for="n in 5" :key="n" :class="[skeletonClass, 'h-12 w-full']" />
      </div>

      <div v-if="!loading || traces.length > 0" class="border-2 border-border overflow-hidden">
        <table class="w-full">
          <thead class="sticky top-0 z-10">
            <tr :class="rowClass">
              <th :class="[thClass, 'text-left']">Trace ID</th>
              <th :class="[thClass, 'text-left']">Operation</th>
              <th :class="[thClass, 'text-left']">Services</th>
              <th :class="[thClass, 'text-right']">Duration</th>
              <th :class="[thClass, 'text-right']">Spans</th>
              <th :class="[thClass, 'text-center']">Status</th>
              <th :class="[thClass, 'text-right']">Time</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="!loading && !error && traces.length === 0">
              <td colspan="7" class="px-5 py-12 text-center text-text-secondary text-sm">
                No traces found. Waiting for telemetry data...
              </td>
            </tr>
            <tr
              v-for="trace in traces"
              :key="trace.trace_id"
              data-testid="trace-row"
              :class="[rowClass, 'cursor-pointer animate-fade-in']"
              @click="openTrace(trace.trace_id)"
            >
              <td class="px-5 py-5">
                <span
                  data-testid="trace-id"
                  class="font-mono text-sm text-accent hover:text-accent-hover"
                >
                  {{ truncateId(trace.trace_id) }}
                </span>
              </td>
              <td class="px-5 py-5 text-sm text-text-secondary max-w-[200px] truncate">
                {{ trace.root_operation || "(unknown)" }}
              </td>
              <td class="px-5 py-5">
                <div class="flex flex-wrap gap-1">
                  <span v-for="svc in trace.services" :key="svc" :class="badgeClass('default')">{{
                    svc
                  }}</span>
                </div>
              </td>
              <td class="px-5 py-5 text-right">
                <span
                  class="text-sm font-mono"
                  :class="trace.duration_ms > 1000 ? 'text-warning' : 'text-text-secondary'"
                >
                  {{ formatDuration(trace.duration_ms) }}
                </span>
              </td>
              <td class="px-5 py-5 text-right text-sm text-text-secondary">
                {{ trace.span_count }}
              </td>
              <td class="px-5 py-5 text-center">
                <span
                  v-if="trace.http_status"
                  data-testid="trace-status-badge"
                  :class="badgeClass(httpStatusVariant(trace.http_status))"
                >
                  {{ trace.http_status }}
                </span>
                <span
                  v-else
                  data-testid="trace-status-badge"
                  :class="badgeClass(trace.has_error ? 'error' : 'success')"
                >
                  {{ trace.has_error ? "Error" : "Ok" }}
                </span>
              </td>
              <td class="px-5 py-5 text-right text-xs text-text-secondary font-mono">
                {{ formatTime(trace.start_time) }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
