<script setup lang="ts">
// Port of dashboard/src/components/MetricChart.tsx (uPlot-based) to a
// dependency-free SVG renderer. Accepts uPlot-style aligned data:
// [timestamps_in_seconds, ...series_value_arrays].
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    data: number[][];
    width: number;
    height: number;
    title?: string;
    seriesLabels?: string[];
    chartType?: "line" | "bar";
  }>(),
  { chartType: "line" },
);

// Color palette for multi-series (matches the SolidJS original)
const SERIES_COLORS = [
  "#FFD600", // yellow (accent)
  "#4ADE80", // green (success)
  "#60a5fa", // blue (info)
  "#fbbf24", // amber (warning)
  "#f87171", // red (error)
  "#a78bfa", // violet
  "#2dd4bf", // teal
  "#fb923c", // orange
];

const PAD = { top: 10, right: 10, bottom: 24, left: 60 };

const xs = computed(() => props.data[0] ?? []);
const seriesValues = computed(() => props.data.slice(1));

const plotW = computed(() => Math.max(props.width - PAD.left - PAD.right, 1));
const plotH = computed(() => Math.max(props.height - PAD.top - PAD.bottom, 1));

const xDomain = computed<readonly [number, number]>(() => {
  const v = xs.value;
  if (v.length === 0) return [0, 1];
  let min = Math.min(...v);
  let max = Math.max(...v);
  if (min === max) {
    min -= 1;
    max += 1;
  }
  return [min, max];
});

const yDomain = computed<readonly [number, number]>(() => {
  let min = Infinity;
  let max = -Infinity;
  for (const s of seriesValues.value) {
    for (const v of s) {
      if (v < min) min = v;
      if (v > max) max = v;
    }
  }
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [0, 1];
  min = Math.min(min, 0);
  if (min === max) max = min + 1;
  return [min, max];
});

function xPos(t: number): number {
  const [min, max] = xDomain.value;
  return PAD.left + ((t - min) / (max - min)) * plotW.value;
}

function yPos(v: number): number {
  const [min, max] = yDomain.value;
  return PAD.top + (1 - (v - min) / (max - min)) * plotH.value;
}

function colorFor(i: number): string {
  return SERIES_COLORS[i % SERIES_COLORS.length] ?? SERIES_COLORS[0]!;
}

function labelFor(i: number): string {
  return props.seriesLabels?.[i] ?? `Series ${i + 1}`;
}

function formatTick(v: number): string {
  if (Math.abs(v) >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`;
  if (Math.abs(v) >= 1_000) return `${(v / 1_000).toFixed(1)}K`;
  if (Number.isInteger(v)) return String(v);
  return v.toFixed(2);
}

const yTicks = computed(() => {
  const [min, max] = yDomain.value;
  const ticks: { y: number; label: string }[] = [];
  const count = 4;
  for (let i = 0; i <= count; i++) {
    const v = min + ((max - min) * i) / count;
    ticks.push({ y: yPos(v), label: formatTick(v) });
  }
  return ticks;
});

const xTicks = computed(() => {
  const v = xs.value;
  if (v.length === 0) return [];
  const count = Math.min(4, Math.max(v.length - 1, 1));
  const ticks: { t: number; x: number; label: string }[] = [];
  const seen = new Set<number>();
  for (let i = 0; i <= count; i++) {
    const idx = Math.round(((v.length - 1) * i) / count);
    const t = v[idx];
    if (t === undefined || seen.has(t)) continue;
    seen.add(t);
    ticks.push({
      t,
      x: xPos(t),
      label: new Date(t * 1000).toLocaleTimeString(undefined, { hour12: false }),
    });
  }
  return ticks;
});

function linePoints(values: number[]): string {
  return xs.value
    .map((t, i) => `${xPos(t).toFixed(1)},${yPos(values[i] ?? 0).toFixed(1)}`)
    .join(" ");
}

function areaPoints(values: number[]): string {
  const v = xs.value;
  if (v.length === 0) return "";
  const base = (PAD.top + plotH.value).toFixed(1);
  const first = xPos(v[0] ?? 0).toFixed(1);
  const last = xPos(v[v.length - 1] ?? 0).toFixed(1);
  return `${first},${base} ${linePoints(values)} ${last},${base}`;
}

interface Bar {
  x: number;
  y: number;
  w: number;
  h: number;
  fill: string;
  stroke: string;
}

const bars = computed<Bar[]>(() => {
  if (props.chartType !== "bar") return [];
  const result: Bar[] = [];
  const n = xs.value.length;
  if (n === 0) return result;
  const slot = plotW.value / n;
  const groupW = slot * 0.6;
  const num = Math.max(seriesValues.value.length, 1);
  const barW = Math.max(groupW / num, 1);
  const y0 = yPos(0); // zero baseline (yDomain always includes 0)
  for (let si = 0; si < seriesValues.value.length; si++) {
    const values = seriesValues.value[si] ?? [];
    const color = colorFor(si);
    for (let i = 0; i < n; i++) {
      const yv = yPos(values[i] ?? 0);
      const x = xPos(xs.value[i] ?? 0) - groupW / 2 + si * barW;
      result.push({
        x,
        y: Math.min(yv, y0),
        w: barW,
        h: Math.abs(y0 - yv),
        fill: color + "40",
        stroke: color,
      });
    }
  }
  return result;
});
</script>

<template>
  <div class="overflow-hidden">
    <div v-if="title" class="text-sm text-text-primary font-mono mb-2">{{ title }}</div>
    <svg :width="width" :height="height" class="block">
      <!-- Horizontal grid lines + y-axis labels -->
      <g v-for="(tick, i) in yTicks" :key="'y' + i">
        <line
          :x1="PAD.left"
          :x2="PAD.left + plotW"
          :y1="tick.y"
          :y2="tick.y"
          style="stroke: var(--color-surface-3)"
          stroke-width="1"
        />
        <text
          :x="PAD.left - 8"
          :y="tick.y + 3"
          text-anchor="end"
          style="
            font:
              11px &quot;JetBrains Mono&quot;,
              monospace;
            fill: var(--color-text-secondary);
          "
        >
          {{ tick.label }}
        </text>
      </g>
      <!-- Vertical grid lines + x-axis (time) labels -->
      <g v-for="tick in xTicks" :key="'x' + tick.t">
        <line
          :x1="tick.x"
          :x2="tick.x"
          :y1="PAD.top"
          :y2="PAD.top + plotH"
          style="stroke: var(--color-surface-3)"
          stroke-width="1"
        />
        <text
          :x="tick.x"
          :y="height - 8"
          text-anchor="middle"
          style="
            font:
              11px &quot;JetBrains Mono&quot;,
              monospace;
            fill: var(--color-text-secondary);
          "
        >
          {{ tick.label }}
        </text>
      </g>
      <!-- Series -->
      <template v-if="chartType === 'bar'">
        <rect
          v-for="(bar, i) in bars"
          :key="i"
          :x="bar.x"
          :y="bar.y"
          :width="bar.w"
          :height="bar.h"
          :fill="bar.fill"
          :stroke="bar.stroke"
          stroke-width="1"
        />
      </template>
      <template v-else>
        <g v-for="(values, si) in seriesValues" :key="si">
          <polygon :points="areaPoints(values)" :fill="colorFor(si) + '15'" />
          <polyline
            :points="linePoints(values)"
            fill="none"
            :stroke="colorFor(si)"
            stroke-width="2"
            stroke-linejoin="round"
          />
        </g>
      </template>
    </svg>
    <!-- Legend (only for multi-series, matching the uPlot behavior) -->
    <div v-if="seriesValues.length > 1" class="flex flex-wrap gap-x-4 gap-y-1 mt-1">
      <div v-for="(_values, si) in seriesValues" :key="'l' + si" class="flex items-center gap-1.5">
        <span class="inline-block w-2.5 h-2.5" :style="{ background: colorFor(si) }" />
        <span class="text-xs text-text-secondary font-mono">{{ labelFor(si) }}</span>
      </div>
    </div>
  </div>
</template>
