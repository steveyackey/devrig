<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { ExternalLink } from "@lucide/vue";
import { fetchStatus, fetchServices, type StatusResponse, type ServiceInfo } from "@/api";

const status = ref<StatusResponse | null>(null);
const services = ref<ServiceInfo[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const lastRefresh = ref("");

async function load() {
  try {
    error.value = null;
    const [s, svcs] = await Promise.all([fetchStatus(), fetchServices()]);
    status.value = s;
    services.value = svcs;
    lastRefresh.value = new Date().toLocaleTimeString();
  } catch (err: unknown) {
    error.value = err instanceof Error ? err.message : "Failed to load status";
  } finally {
    loading.value = false;
  }
}

let interval: ReturnType<typeof setInterval> | null = null;
onMounted(() => {
  load();
  interval = setInterval(load, 5000);
});
onUnmounted(() => {
  if (interval) clearInterval(interval);
});

function indicatorClass(svc: ServiceInfo): string {
  const kind = svc.kind ?? "";
  const phase = svc.phase ?? "";
  if (kind === "addon" || kind === "cluster-port") return "bg-success";
  if (phase === "stopped" || phase === "failed") {
    const code = svc.exit_code;
    return phase === "failed" || (code != null && code !== 0) ? "bg-error" : "bg-surface-3";
  }
  return phase === "running" ? "bg-success animate-pulse-live" : "bg-warning animate-pulse-live";
}

function indicatorStyle(svc: ServiceInfo): string {
  const kind = svc.kind ?? "";
  const phase = svc.phase ?? "";
  if (kind === "link") return "background-color:#60a5fa;box-shadow:0 0 6px rgba(96,165,250,0.3)";
  if (kind === "addon" || kind === "cluster-port") return "box-shadow:0 0 6px rgba(74,222,128,0.3)";
  if (phase === "stopped" || phase === "failed") {
    const code = svc.exit_code;
    return phase === "failed" || (code != null && code !== 0)
      ? "box-shadow:0 0 6px rgba(239,68,68,0.4)"
      : "";
  }
  return phase === "running"
    ? "box-shadow:0 0 6px rgba(74,222,128,0.3)"
    : "box-shadow:0 0 6px rgba(251,191,36,0.3)";
}
</script>

<template>
  <div data-testid="status-view" class="flex flex-col h-full">
    <div class="px-8 py-6 border-b-2 border-border flex items-start justify-between">
      <div>
        <h2
          class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
          style="text-shadow: 2px 2px 0 rgba(0, 0, 0, 0.5)"
        >
          System Status
        </h2>
        <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">
          Telemetry pipeline overview
        </p>
      </div>
      <div class="flex items-center gap-4">
        <span
          v-if="lastRefresh"
          class="font-label text-[9px] text-text-secondary uppercase tracking-[0.08em]"
        >
          Last refresh: {{ lastRefresh }}
        </span>
        <button
          class="border border-border bg-surface-1 hover:border-accent/40 text-text-primary text-[11px] px-3 py-1.5 font-label uppercase tracking-[0.1em] cursor-pointer"
          @click="
            loading = true;
            load();
          "
        >
          Refresh
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-auto p-7">
      <div v-if="error" class="mb-6 border-2 border-error/30 bg-error/5 p-4 text-center">
        <p class="text-error text-sm">{{ error }}</p>
        <button
          class="mt-2 text-accent hover:text-accent-hover text-sm border-none bg-transparent cursor-pointer"
          @click="
            loading = true;
            load();
          "
        >
          Retry
        </button>
      </div>

      <div v-if="loading && !status" class="space-y-4">
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          <div
            v-for="n in 4"
            :key="n"
            class="h-28 bg-surface-2 animate-skeleton bg-gradient-to-r from-surface-2 via-surface-3 to-surface-2 bg-[length:200%_100%]"
          />
        </div>
        <div class="h-48 bg-surface-2 animate-skeleton" />
      </div>

      <div v-if="status" class="space-y-7 animate-fade-in">
        <!-- Stat Cards -->
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          <a
            data-testid="stat-card"
            href="#/traces"
            class="bg-surface-1 p-6 border-2 border-border relative hover:border-accent/40 transition-colors block no-underline cursor-pointer"
          >
            <div
              class="absolute top-2.5 right-3 flex items-center gap-1 font-label text-[9px] text-success tracking-[0.06em]"
            >
              <span class="text-[8px]">▲</span> OK
            </div>
            <div class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em] mb-1.5">
              Traces
            </div>
            <div
              data-testid="stat-card-value"
              class="font-display text-[56px] leading-none text-accent"
              style="text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.5)"
            >
              {{ status.trace_count.toLocaleString() }}
            </div>
            <div class="font-label text-[9px] text-text-secondary mt-1">distributed</div>
          </a>
          <a
            data-testid="stat-card"
            href="#/traces"
            class="bg-surface-1 p-6 border-2 border-border relative hover:border-accent/40 transition-colors block no-underline cursor-pointer"
          >
            <div
              class="absolute top-2.5 right-3 flex items-center gap-1 font-label text-[9px] text-success tracking-[0.06em]"
            >
              <span class="text-[8px]">▲</span> OK
            </div>
            <div class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em] mb-1.5">
              Spans
            </div>
            <div
              data-testid="stat-card-value"
              class="font-display text-[56px] leading-none text-accent"
              style="text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.5)"
            >
              {{ status.span_count.toLocaleString() }}
            </div>
            <div class="font-label text-[9px] text-text-secondary mt-1">total</div>
          </a>
          <a
            data-testid="stat-card"
            href="#/logs"
            class="bg-surface-1 p-6 border-2 border-border relative hover:border-accent/40 transition-colors block no-underline cursor-pointer"
          >
            <div
              class="absolute top-2.5 right-3 flex items-center gap-1 font-label text-[9px] text-success tracking-[0.06em]"
            >
              <span class="text-[8px]">▲</span> OK
            </div>
            <div class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em] mb-1.5">
              Logs
            </div>
            <div
              data-testid="stat-card-value"
              class="font-display text-[56px] leading-none text-accent"
              style="text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.5)"
            >
              {{ status.log_count.toLocaleString() }}
            </div>
            <div class="font-label text-[9px] text-text-secondary mt-1">records</div>
          </a>
          <a
            data-testid="stat-card"
            href="#/metrics"
            class="bg-surface-1 p-6 border-2 border-border relative hover:border-accent/40 transition-colors block no-underline cursor-pointer"
          >
            <div
              class="absolute top-2.5 right-3 flex items-center gap-1 font-label text-[9px] text-success tracking-[0.06em]"
            >
              <span class="text-[8px]">▲</span> OK
            </div>
            <div class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em] mb-1.5">
              Metrics
            </div>
            <div
              data-testid="stat-card-value"
              class="font-display text-[56px] leading-none text-accent"
              style="text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.5)"
            >
              {{ status.metric_count.toLocaleString() }}
            </div>
            <div class="font-label text-[9px] text-text-secondary mt-1">points</div>
          </a>
        </div>

        <!-- Services table -->
        <div class="border-2 border-border bg-surface-1">
          <div class="px-6 py-4 border-b border-border flex items-center justify-between">
            <h3 class="font-display text-[22px] text-accent tracking-[0.1em] uppercase">
              Services ({{ services.length || status.services.length }})
            </h3>
            <span class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em]"
              >Configured services and their ports</span
            >
          </div>

          <div
            v-if="services.length === 0 && status.services.length === 0"
            class="px-6 py-8 text-center text-text-secondary text-sm"
          >
            No services reporting yet.
          </div>

          <!-- Rich service list from /api/services -->
          <div v-if="services.length > 0">
            <div
              v-for="svc in services"
              :key="svc.name"
              data-testid="service-row"
              class="px-6 py-3.5 flex items-center gap-3.5 border-b border-border last:border-b-0 hover:bg-accent/[0.03] transition-colors"
            >
              <span
                data-testid="service-indicator"
                class="inline-block w-2 h-2 rounded-full border-solid"
                :class="indicatorClass(svc)"
                :style="indicatorStyle(svc)"
              />
              <span class="font-display text-lg text-text-primary tracking-[0.06em] uppercase">{{
                svc.name
              }}</span>
              <span
                v-if="svc.kind"
                class="text-[9px] font-label text-text-muted border border-border px-1.5 py-0.5 uppercase tracking-[0.08em]"
                >{{ svc.kind }}</span
              >
              <span
                v-if="svc.addon_type"
                class="text-[9px] font-label text-text-muted border border-border px-1.5 py-0.5 uppercase tracking-[0.08em]"
                >{{ svc.addon_type }}</span
              >
              <template v-if="svc.phase === 'stopped' || svc.phase === 'failed'">
                <span
                  class="text-[9px] font-label border px-1.5 py-0.5 uppercase tracking-[0.08em]"
                  :class="
                    svc.phase === 'failed' || (svc.exit_code != null && svc.exit_code !== 0)
                      ? 'text-error border-error/30'
                      : 'text-text-muted border-border'
                  "
                >
                  {{
                    svc.phase === "failed" || (svc.exit_code != null && svc.exit_code !== 0)
                      ? `exited (${svc.exit_code ?? "?"})`
                      : "exited (0)"
                  }}
                </span>
              </template>
              <template v-if="svc.port">
                <a
                  v-if="!svc.protocol || svc.protocol === 'http' || svc.protocol === 'https'"
                  :href="svc.url || `${svc.protocol ?? 'http'}://localhost:${svc.port}`"
                  target="_blank"
                  rel="noopener"
                  class="inline-flex items-center gap-1 text-xs font-mono text-text-muted hover:text-accent transition-colors no-underline"
                >
                  :{{ svc.port }}
                  <ExternalLink :size="10" />
                </a>
                <span
                  v-else
                  class="inline-flex items-center gap-1 text-xs font-mono text-text-muted"
                >
                  :{{ svc.port }}
                  <span class="text-[9px] font-label border border-border px-1 uppercase">{{
                    svc.protocol
                  }}</span>
                </span>
                <span v-if="svc.port_auto" class="text-[10px] text-text-muted">(auto)</span>
              </template>
              <div
                v-if="svc.kind !== 'addon' && svc.kind !== 'cluster-port' && svc.kind !== 'link'"
                class="ml-auto flex gap-2.5"
              >
                <RouterLink
                  to="/traces"
                  class="font-label text-[9px] text-text-muted hover:text-accent uppercase tracking-[0.08em] transition-colors no-underline"
                  >Traces</RouterLink
                >
                <RouterLink
                  to="/logs"
                  class="font-label text-[9px] text-text-muted hover:text-accent uppercase tracking-[0.08em] transition-colors no-underline"
                  >Logs</RouterLink
                >
              </div>
            </div>
          </div>

          <!-- Fallback: service names from status -->
          <div v-else-if="status.services.length > 0">
            <div
              v-for="svcName in status.services"
              :key="svcName"
              data-testid="service-row"
              class="px-6 py-3.5 flex items-center gap-3.5 border-b border-border last:border-b-0 hover:bg-accent/[0.03] transition-colors"
            >
              <span
                data-testid="service-indicator"
                class="inline-block w-2 h-2 rounded-full border-solid bg-success animate-pulse-live"
                style="box-shadow: 0 0 6px rgba(74, 222, 128, 0.3)"
              />
              <span class="font-display text-lg text-text-primary tracking-[0.06em] uppercase">{{
                svcName
              }}</span>
              <div class="ml-auto flex gap-2.5">
                <RouterLink
                  to="/traces"
                  class="font-label text-[9px] text-text-muted hover:text-accent uppercase tracking-[0.08em] transition-colors no-underline"
                  >Traces</RouterLink
                >
                <RouterLink
                  to="/logs"
                  class="font-label text-[9px] text-text-muted hover:text-accent uppercase tracking-[0.08em] transition-colors no-underline"
                  >Logs</RouterLink
                >
              </div>
            </div>
          </div>
        </div>

        <div class="text-center">
          <p class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em]">
            Auto-refreshes every 5 seconds
          </p>
        </div>
      </div>
    </div>
  </div>
</template>
