<script setup lang="ts">
// Port of dashboard/src/views/MetricsView.tsx (SolidJS) to Vue 3.
import { ref, computed, inject, watch, onMounted, onBeforeUnmount } from "vue";
import type { Ref } from "vue";
import {
  fetchMetrics,
  fetchMetricSeries,
  fetchStatus,
  type StoredMetric,
  type MetricSeries,
  type TelemetryEvent,
} from "@/api";
import MetricChart from "@/components/MetricChart.vue";
import Sparkline from "@/components/Sparkline.vue";
import { formatTime, formatValue } from "@/lib/format";

interface MetricCard {
  name: string;
  type: string;
  unit: string | null;
  latestValue: number;
  services: string[];
  sparklineData: [number[], number[]] | null;
}

const latestEvent = inject<Ref<TelemetryEvent | null>>("latestEvent");

const metrics = ref<StoredMetric[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const services = ref<string[]>([]);
const selectedMetric = ref<string | null>(null);
const chartSeries = ref<MetricSeries[]>([]);
const chartLoading = ref(false);

// Streaming
const streaming = ref(true);

const filterName = ref("");
const filterService = ref("");
const filterType = ref("");

async function loadMetrics() {
  try {
    error.value = null;
    metrics.value = await fetchMetrics({
      name: filterName.value || undefined,
      metric_type: filterType.value || undefined,
      service: filterService.value || undefined,
      limit: 200,
    });
  } catch (err: unknown) {
    error.value = err instanceof Error ? err.message : "Failed to load metrics";
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
  loadMetrics();
  loadServices();
});

watch(latestEvent ?? ref(null), (event) => {
  if (event && event.type === "MetricUpdate" && streaming.value) {
    loadMetrics();
  }
});

// Build metric cards from raw data
const metricCards = computed((): MetricCard[] => {
  const grouped = new Map<string, StoredMetric[]>();
  for (const metric of metrics.value) {
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
      type: latest?.metric_type ?? "Gauge",
      unit: latest?.unit ?? null,
      latestValue: latest?.value ?? 0,
      services: svcs,
      sparklineData,
    };
  });
});

// Load chart data when a metric is selected
async function loadChartData(metricName: string) {
  chartLoading.value = true;
  try {
    const resp = await fetchMetricSeries(metricName, filterService.value || undefined);
    chartSeries.value = resp.series;
  } catch {
    chartSeries.value = [];
  } finally {
    chartLoading.value = false;
  }
}

function handleCardClick(name: string) {
  if (selectedMetric.value === name) {
    selectedMetric.value = null;
    chartSeries.value = [];
  } else {
    selectedMetric.value = name;
    loadChartData(name);
  }
}

function closeChart() {
  selectedMetric.value = null;
  chartSeries.value = [];
}

// Build aligned chart data [timestamps_in_seconds, ...values] from chart series
const chartData = computed((): [number[], ...number[][]] | null => {
  const series = chartSeries.value;
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

const seriesLabels = computed(() => chartSeries.value.map((s) => s.service_name));

const chartType = computed((): "line" | "bar" => {
  const type = chartSeries.value[0]?.metric_type ?? "Gauge";
  return type === "Histogram" ? "bar" : "line";
});

// Track the chart container width (ResizeObserver, as in the SolidJS original)
const chartContainerRef = ref<HTMLDivElement | null>(null);
const chartWidth = ref(600);
let resizeObserver: ResizeObserver | null = null;

watch(chartContainerRef, (el) => {
  resizeObserver?.disconnect();
  resizeObserver = null;
  if (el) {
    chartWidth.value = el.clientWidth;
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        chartWidth.value = entry.contentRect.width;
      }
    });
    resizeObserver.observe(el);
  }
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  resizeObserver = null;
});

function handleSearch() {
  loading.value = true;
  loadMetrics();
}

function handleClear() {
  filterName.value = "";
  filterType.value = "";
  filterService.value = "";
  loading.value = true;
  loadMetrics();
}

function handleRetry() {
  loading.value = true;
  loadMetrics();
}

const metricsCountText = computed(
  () => `${metrics.value.length} metric${metrics.value.length !== 1 ? "s" : ""}`,
);

// Badge classes ported from the SolidJS ui/badge.tsx variants.
const badgeBase =
  "inline-flex items-center px-2 py-0.5 text-[9px] font-label uppercase tracking-wider transition-colors";

function metricTypeBadgeClass(type: string): string {
  switch (type) {
    case "Counter":
      return `${badgeBase} text-info border border-info/30`;
    case "Gauge":
      return `${badgeBase} text-success border border-success/30`;
    case "Histogram":
      return `${badgeBase} text-[#a855f7] border border-[#a855f7]/30`;
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
  "bg-surface-1 text-[10px] font-label text-text-muted uppercase tracking-[0.15em] px-5 py-3";
const skeletonClass =
  "bg-gradient-to-r from-surface-2 via-surface-3 to-surface-2 bg-[length:400%_100%] animate-skeleton";
</script>

<template>
  <div data-testid="metrics-view" class="flex flex-col h-full">
    <!-- Header -->
    <div class="px-8 py-6 border-b-2 border-border">
      <h2
        class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
        style="text-shadow: 2px 2px 0 rgba(0, 0, 0, 0.5)"
      >
        Metrics
      </h2>
      <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">
        Telemetry metric data points
      </p>
    </div>

    <!-- Filter Bar -->
    <form
      class="px-7 py-4 border-b-2 border-border flex items-center gap-4 flex-wrap"
      @submit.prevent="handleSearch"
    >
      <div class="flex items-center gap-2">
        <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]"
          >Metric Name</label
        >
        <input
          v-model="filterName"
          type="text"
          placeholder="Filter by name..."
          :class="[inputClass, 'w-48']"
        />
      </div>

      <div class="flex items-center gap-2">
        <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]"
          >Type</label
        >
        <select v-model="filterType" :class="[selectClass, 'min-w-[120px]']">
          <option value="">All</option>
          <option value="Gauge">Gauge</option>
          <option value="Counter">Counter</option>
          <option value="Histogram">Histogram</option>
        </select>
      </div>

      <div class="flex items-center gap-2">
        <label class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em]"
          >Service</label
        >
        <select v-model="filterService" :class="[selectClass, 'min-w-[140px]']">
          <option value="">All Services</option>
          <option v-for="svc in services" :key="svc" :value="svc">{{ svc }}</option>
        </select>
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

      <div data-testid="metrics-count" class="text-xs text-text-secondary">
        {{ metricsCountText }}
      </div>
    </form>

    <div class="flex-1 overflow-auto p-7">
      <div v-if="error" class="py-8 text-center">
        <p class="text-error text-sm">{{ error }}</p>
        <button class="mt-2 text-accent hover:text-accent-hover text-sm" @click="handleRetry">
          Retry
        </button>
      </div>

      <div v-if="loading && metrics.length === 0" class="py-6 space-y-4">
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-5">
          <div v-for="n in 3" :key="n" :class="[skeletonClass, 'h-28 rounded-lg']" />
        </div>
      </div>

      <div v-if="!loading || metrics.length > 0" class="space-y-6 animate-fade-in">
        <!-- Metric Cards Grid -->
        <div
          v-if="metricCards.length > 0"
          class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-5"
        >
          <button
            v-for="card in metricCards"
            :key="card.name"
            data-testid="metric-card"
            type="button"
            class="text-left border-2 p-6 transition-all hover:border-border-hover overflow-hidden"
            :class="
              selectedMetric === card.name
                ? 'border-accent/40 bg-accent/5'
                : 'border-border bg-surface-1'
            "
            @click="handleCardClick(card.name)"
          >
            <div class="flex items-start justify-between mb-2">
              <span
                class="text-xs font-mono text-text-secondary truncate max-w-[70%]"
                :title="card.name"
                >{{ card.name }}</span
              >
              <span :class="[metricTypeBadgeClass(card.type), 'text-[10px] shrink-0 ml-2']">{{
                card.type
              }}</span>
            </div>
            <div class="flex items-end justify-between">
              <div>
                <span class="text-2xl font-semibold font-mono text-text-primary">{{
                  formatValue(card.latestValue)
                }}</span>
                <span v-if="card.unit" class="text-xs text-text-secondary ml-1">{{
                  card.unit
                }}</span>
              </div>
              <div v-if="card.sparklineData" class="ml-2 shrink-0 overflow-hidden">
                <Sparkline :data="card.sparklineData" :width="72" :height="28" />
              </div>
            </div>
            <div class="mt-2 flex flex-wrap gap-1">
              <span
                v-for="svc in card.services"
                :key="svc"
                class="text-[10px] text-text-secondary bg-surface-2 rounded px-1.5 py-0.5"
                >{{ svc }}</span
              >
            </div>
          </button>
        </div>

        <!-- Expanded Chart Panel -->
        <div v-if="selectedMetric" class="border-2 border-border bg-surface-1 p-6 overflow-hidden">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h3 class="text-sm font-semibold text-text-primary font-mono">
                {{ selectedMetric }}
              </h3>
              <p class="text-xs text-text-secondary mt-0.5">{{ chartSeries.length }} series</p>
            </div>
            <button
              type="button"
              class="text-xs text-text-secondary hover:text-text-primary px-2 py-1 rounded hover:bg-surface-2"
              @click="closeChart"
            >
              Close
            </button>
          </div>
          <div ref="chartContainerRef">
            <div v-if="chartLoading" :class="[skeletonClass, 'h-[300px] w-full rounded']" />
            <MetricChart
              v-else-if="chartData"
              :data="chartData"
              :width="chartWidth"
              :height="300"
              :series-labels="seriesLabels"
              :chart-type="chartType"
            />
            <div
              v-else
              class="h-[300px] flex items-center justify-center text-text-secondary text-sm"
            >
              No time-series data available for this metric.
            </div>
          </div>
        </div>

        <!-- Data Table -->
        <div class="border-2 border-border overflow-hidden">
          <table class="w-full">
            <thead class="sticky top-0 z-10">
              <tr class="border-b border-border hover:bg-accent/[0.03] transition-colors">
                <th :class="[thClass, 'text-left']">Time</th>
                <th :class="[thClass, 'text-left']">Service</th>
                <th :class="[thClass, 'text-left']">Metric Name</th>
                <th :class="[thClass, 'text-left']">Type</th>
                <th :class="[thClass, 'text-right']">Value</th>
                <th :class="[thClass, 'text-left']">Unit</th>
              </tr>
            </thead>
            <tbody>
              <tr v-if="!loading && !error && metrics.length === 0">
                <td colspan="6" class="px-5 py-12 text-center text-text-secondary text-sm">
                  No metrics found. Adjust filters or wait for new data.
                </td>
              </tr>
              <tr
                v-for="metric in metrics"
                :key="metric.record_id"
                data-testid="metric-row"
                class="border-b border-border hover:bg-accent/[0.03] transition-colors animate-fade-in"
              >
                <td class="px-5 py-5 text-xs font-mono text-text-secondary whitespace-nowrap">
                  {{ formatTime(metric.timestamp) }}
                </td>
                <td class="px-5 py-5 text-sm text-text-secondary">
                  {{ metric.service_name }}
                </td>
                <td class="px-5 py-5">
                  <button
                    data-testid="metric-name"
                    type="button"
                    class="text-sm font-mono text-accent hover:text-accent-hover cursor-pointer"
                    @click="handleCardClick(metric.metric_name)"
                  >
                    {{ metric.metric_name }}
                  </button>
                </td>
                <td class="px-5 py-5">
                  <span
                    data-testid="metric-type-badge"
                    :class="metricTypeBadgeClass(metric.metric_type)"
                    >{{ metric.metric_type }}</span
                  >
                </td>
                <td class="px-5 py-5 text-right">
                  <span data-testid="metric-value" class="text-sm font-mono text-text-primary">{{
                    formatValue(metric.value)
                  }}</span>
                </td>
                <td class="px-5 py-5 text-sm text-text-secondary">
                  {{ metric.unit ?? "-" }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>
  </div>
</template>
