<script setup lang="ts">
// Sparkline variant — minimal, no axes. Port of the `Sparkline` named export
// from dashboard/src/components/MetricChart.tsx, rendered as SVG instead of uPlot.
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    data: number[][]; // [timestamps_in_seconds, values]
    width: number;
    height: number;
    color?: string;
  }>(),
  { color: "#FFD600" },
);

const PAD = 2;

const xs = computed(() => props.data[0] ?? []);
const values = computed(() => props.data[1] ?? []);

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
  const v = values.value;
  if (v.length === 0) return [0, 1];
  let min = Math.min(...v);
  let max = Math.max(...v);
  if (min === max) {
    min -= 1;
    max += 1;
  }
  return [min, max];
});

function xPos(t: number): number {
  const [min, max] = xDomain.value;
  return PAD + ((t - min) / (max - min)) * Math.max(props.width - PAD * 2, 1);
}

function yPos(v: number): number {
  const [min, max] = yDomain.value;
  return PAD + (1 - (v - min) / (max - min)) * Math.max(props.height - PAD * 2, 1);
}

const linePoints = computed(() =>
  xs.value
    .map((t, i) => `${xPos(t).toFixed(1)},${yPos(values.value[i] ?? 0).toFixed(1)}`)
    .join(" "),
);

const areaPoints = computed(() => {
  const v = xs.value;
  if (v.length === 0) return "";
  const base = (props.height - PAD).toFixed(1);
  const first = xPos(v[0] ?? 0).toFixed(1);
  const last = xPos(v[v.length - 1] ?? 0).toFixed(1);
  return `${first},${base} ${linePoints.value} ${last},${base}`;
});
</script>

<template>
  <div class="overflow-hidden">
    <svg :width="width" :height="height" class="block">
      <polygon :points="areaPoints" :fill="color + '20'" />
      <polyline
        :points="linePoints"
        fill="none"
        :stroke="color"
        stroke-width="1.5"
        stroke-linejoin="round"
      />
    </svg>
  </div>
</template>
