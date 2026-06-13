<script setup lang="ts">
import { ref, onMounted, onUnmounted, provide } from "vue";
import { RouterView } from "vue-router";
import { createWebSocket } from "@/lib/ws";
import { initTheme } from "@/lib/theme";
import type { TelemetryEvent } from "@/api";
import Sidebar from "@/components/Sidebar.vue";
import StatusBar from "@/components/StatusBar.vue";
import CommandPalette from "@/components/CommandPalette.vue";

initTheme();

const latestEvent = ref<TelemetryEvent | null>(null);
const wsConnected = ref(false);

// Provide event stream to child views.
provide("latestEvent", latestEvent);
provide("wsConnected", wsConnected);

let wsClient: ReturnType<typeof createWebSocket> | null = null;

onMounted(() => {
  wsClient = createWebSocket({
    onEvent: (ev) => {
      latestEvent.value = ev;
    },
    onStatusChange: (connected) => {
      wsConnected.value = connected;
    },
  });
});

onUnmounted(() => {
  wsClient?.close();
});
</script>

<template>
  <div
    data-testid="app-layout"
    class="flex flex-col h-screen bg-surface-0 text-text-primary font-sans"
  >
    <div class="flex flex-1 min-h-0 max-[960px]:flex-col">
      <Sidebar />
      <main data-testid="main-content" class="flex-1 overflow-hidden bg-surface-0 stencil-bg">
        <RouterView />
      </main>
    </div>
    <StatusBar :ws-connected="wsConnected" />
    <CommandPalette />
  </div>
</template>
