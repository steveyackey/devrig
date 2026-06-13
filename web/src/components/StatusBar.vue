<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { fetchStatus, type StatusResponse } from "@/api";

// App.vue passes :ws-connected, but (like the original status bar) we derive
// connectivity by polling `window.__devrig_ws_connected` so that direct
// mutations of the flag (e.g. by E2E tests) are reflected within ~2s.
defineProps<{ wsConnected?: boolean }>();

const status = ref<StatusResponse | null>(null);
const wsLive = ref(false);
const lastUpdated = ref("");

let statusInterval: ReturnType<typeof setInterval> | null = null;
let wsInterval: ReturnType<typeof setInterval> | null = null;

async function loadStatus() {
  try {
    status.value = await fetchStatus();
    lastUpdated.value = new Date().toLocaleTimeString();
  } catch {
    // silently handle - status bar is non-critical
  }
}

function checkWs() {
  wsLive.value = Boolean((window as unknown as Record<string, unknown>)["__devrig_ws_connected"]);
}

onMounted(() => {
  loadStatus();
  statusInterval = setInterval(loadStatus, 10000);

  checkWs();
  wsInterval = setInterval(checkWs, 2000);
});

onUnmounted(() => {
  if (statusInterval) clearInterval(statusInterval);
  if (wsInterval) clearInterval(wsInterval);
});
</script>

<template>
  <footer
    data-testid="status-bar"
    class="h-10 bg-surface-0 border-t-2 border-border flex items-center px-6 text-[9px] font-label text-text-secondary gap-4 shrink-0"
  >
    <span class="flex items-center gap-1.5">
      <span
        data-testid="status-bar-ws-indicator"
        class="inline-block w-1.5 h-1.5 rounded-full border-solid"
        :class="wsLive ? 'bg-success animate-pulse-live' : 'bg-surface-3'"
        :style="wsLive ? 'box-shadow: 0 0 4px rgba(74,222,128,0.4)' : ''"
      />
      <span data-testid="status-bar-ws-status" :class="wsLive ? 'text-success' : 'text-text-muted'">
        {{ wsLive ? "Live" : "Disconnected" }}
      </span>
    </span>

    <template v-if="status">
      <span class="text-accent/10" aria-hidden="true">&middot;</span>
      <span
        >Traces:
        <span data-testid="status-bar-traces-count">{{
          status.trace_count.toLocaleString()
        }}</span></span
      >
      <span class="text-accent/10" aria-hidden="true">&middot;</span>
      <span
        >Spans:
        <span data-testid="status-bar-spans-count">{{
          status.span_count.toLocaleString()
        }}</span></span
      >
      <span class="text-accent/10" aria-hidden="true">&middot;</span>
      <span
        >Logs:
        <span data-testid="status-bar-logs-count">{{
          status.log_count.toLocaleString()
        }}</span></span
      >
      <span class="text-accent/10" aria-hidden="true">&middot;</span>
      <span
        >Metrics:
        <span data-testid="status-bar-metrics-count">{{
          status.metric_count.toLocaleString()
        }}</span></span
      >
      <span class="text-accent/10" aria-hidden="true">&middot;</span>
      <span
        >Services:
        <span data-testid="status-bar-services-count">{{ status.services.length }}</span></span
      >
    </template>

    <span class="ml-auto text-text-muted">{{ lastUpdated }}</span>
  </footer>
</template>
