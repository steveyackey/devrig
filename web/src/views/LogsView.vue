<script setup lang="ts">
// Port of dashboard/src/views/LogsView.tsx (SolidJS) to Vue 3.
import { ref, computed, inject, watch, onMounted } from "vue";
import type { Ref } from "vue";
import { fetchLogs, fetchStatus, type StoredLog, type TelemetryEvent } from "@/api";
import { formatTimeMs, severityVariant } from "@/lib/format";

const latestEvent = inject<Ref<TelemetryEvent | null>>("latestEvent");

const logs = ref<StoredLog[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const services = ref<string[]>([]);

// Streaming
const streaming = ref(true);

const filterService = ref("");
const filterSeverity = ref("");
const filterSearch = ref("");
const filterSource = ref("");

const severities = ["Trace", "Debug", "Info", "Warn", "Error", "Fatal"];

async function loadLogs() {
  try {
    error.value = null;
    logs.value = await fetchLogs({
      service: filterService.value || undefined,
      severity: filterSeverity.value || undefined,
      search: filterSearch.value || undefined,
      source: filterSource.value || undefined,
      limit: 200,
    });
  } catch (err: unknown) {
    error.value = err instanceof Error ? err.message : "Failed to load logs";
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

onMounted(() => {
  loadLogs();
  loadServices();
});

watch(latestEvent ?? ref(null), (event) => {
  if (event && event.type === "LogRecord" && streaming.value) {
    loadLogs();
  }
});

function handleSearch() {
  loading.value = true;
  loadLogs();
}

function handleClear() {
  filterService.value = "";
  filterSeverity.value = "";
  filterSearch.value = "";
  filterSource.value = "";
  loading.value = true;
  loadLogs();
}

function handleRetry() {
  loading.value = true;
  loadLogs();
}

function getLogSource(log: StoredLog): string {
  const attr = log.attributes.find(([k]) => k === "log.source");
  return attr ? attr[1] : "";
}

function sourceLabel(source: string): string {
  switch (source) {
    case "stdout":
      return "stdout";
    case "stderr":
      return "stderr";
    case "docker":
      return "docker";
    case "otlp":
      return "sdk";
    default:
      return source || "-";
  }
}

const logsCountText = computed(
  () => `${logs.value.length} log${logs.value.length !== 1 ? "s" : ""}`,
);

// Badge classes ported from the SolidJS ui/badge.tsx variants.
const badgeBase =
  "inline-flex items-center px-2 py-0.5 text-[9px] font-label uppercase tracking-wider transition-colors";

function severityBadgeClass(sev: string): string {
  switch (severityVariant(sev)) {
    case "fatal":
      return `${badgeBase} bg-error/20 text-error border border-error/30`;
    case "error":
      return `${badgeBase} text-error border border-error/30`;
    case "warning":
      return `${badgeBase} text-warning border border-warning/30`;
    case "info":
      return `${badgeBase} text-info border border-info/30`;
    case "debug":
      return `${badgeBase} text-text-secondary border border-border`;
    case "trace":
      return `${badgeBase} text-text-muted border border-border`;
    default:
      return `${badgeBase} text-text-muted border border-border`;
  }
}

// Shared class strings ported from the SolidJS ui components.
const selectClass =
  "bg-surface-1 border-2 border-border px-3.5 py-2 text-sm text-text-primary focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent/30";
const inputClass =
  "bg-surface-1 border-2 border-border px-3.5 py-2 text-sm text-text-primary focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent/30 placeholder:text-text-muted";
const buttonClass =
  "inline-flex items-center justify-center gap-2 text-sm font-display tracking-[0.1em] uppercase transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50 disabled:pointer-events-none disabled:opacity-50 border-2 border-accent bg-transparent text-accent hover:bg-accent hover:text-surface-0 h-9 px-4 py-2";
const thClass =
  "bg-surface-1 text-[10px] font-label text-text-muted uppercase tracking-[0.15em] px-5 py-3 text-left";
</script>

<template>
  <div data-testid="logs-view" class="flex flex-col h-full">
    <div class="px-8 py-6 border-b-2 border-border">
      <h2
        class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
        style="text-shadow: 2px 2px 0 rgba(0, 0, 0, 0.5)"
      >
        Logs
      </h2>
      <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">
        Application log records
      </p>
    </div>

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
          >Severity</label
        >
        <select v-model="filterSeverity" :class="[selectClass, 'min-w-[120px]']">
          <option value="">All</option>
          <option v-for="sev in severities" :key="sev" :value="sev">{{ sev }}</option>
        </select>
      </div>

      <div class="flex items-center gap-2">
        <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]"
          >Source</label
        >
        <select v-model="filterSource" :class="[selectClass, 'min-w-[120px]']">
          <option value="">All</option>
          <option value="process">Process</option>
          <option value="docker">Docker</option>
          <option value="otlp">SDK</option>
        </select>
      </div>

      <div class="flex items-center gap-2">
        <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]"
          >Search</label
        >
        <input
          v-model="filterSearch"
          type="text"
          placeholder="Search log body..."
          :class="[inputClass, 'w-60']"
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

      <div data-testid="logs-count" class="text-xs text-text-secondary">{{ logsCountText }}</div>
    </form>

    <div class="flex-1 overflow-auto p-7">
      <div v-if="error" class="py-8 text-center">
        <p class="text-error text-sm">{{ error }}</p>
        <button class="mt-2 text-accent hover:text-accent-hover text-sm" @click="handleRetry">
          Retry
        </button>
      </div>

      <div v-if="loading && logs.length === 0" class="py-4 space-y-2">
        <div
          v-for="n in 5"
          :key="n"
          class="h-10 w-full bg-gradient-to-r from-surface-2 via-surface-3 to-surface-2 bg-[length:400%_100%] animate-skeleton"
        />
      </div>

      <div v-if="!loading || logs.length > 0" class="border-2 border-border overflow-hidden">
        <table class="w-full">
          <thead class="sticky top-0 z-10">
            <tr class="border-b border-border hover:bg-accent/[0.03] transition-colors">
              <th :class="[thClass, 'w-32']">Time</th>
              <th :class="[thClass, 'w-20']">Severity</th>
              <th :class="[thClass, 'w-16']">Source</th>
              <th :class="[thClass, 'w-32']">Service</th>
              <th :class="thClass">Body</th>
              <th :class="[thClass, 'w-32']">Trace</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="!loading && !error && logs.length === 0">
              <td colspan="6" class="px-5 py-12 text-center text-text-secondary text-sm">
                No logs found. Adjust filters or wait for new data.
              </td>
            </tr>
            <tr
              v-for="log in logs"
              :key="log.record_id"
              data-testid="log-row"
              class="border-b border-border hover:bg-accent/[0.03] transition-colors group animate-fade-in"
            >
              <td class="px-5 py-5 align-top">
                <span
                  data-testid="log-timestamp"
                  class="text-xs font-mono text-text-secondary whitespace-nowrap"
                  >{{ formatTimeMs(log.timestamp) }}</span
                >
              </td>
              <td class="px-5 py-5 align-top">
                <span data-testid="log-severity-badge" :class="severityBadgeClass(log.severity)">{{
                  log.severity
                }}</span>
              </td>
              <td class="px-5 py-5 align-top">
                <span
                  data-testid="log-source"
                  class="text-[10px] font-mono text-text-muted uppercase"
                  >{{ sourceLabel(getLogSource(log)) }}</span
                >
              </td>
              <td class="px-5 py-5 text-xs text-text-secondary align-top truncate max-w-[130px]">
                {{ log.service_name }}
              </td>
              <td class="px-5 py-5 text-sm text-text-secondary font-mono align-top">
                <div
                  data-testid="log-body"
                  class="whitespace-pre-wrap break-all max-h-24 overflow-hidden group-hover:max-h-none"
                >
                  {{ log.body }}
                </div>
              </td>
              <td class="px-5 py-5 align-top">
                <a
                  v-if="log.trace_id"
                  data-testid="log-trace-link"
                  :href="`#/traces/${log.trace_id}`"
                  class="text-xs font-mono text-accent hover:text-accent-hover"
                  @click.stop
                  >{{ log.trace_id.slice(0, 8) }}...</a
                >
                <span v-else class="text-xs text-text-secondary">-</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
