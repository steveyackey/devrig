<script setup lang="ts">
import { computed } from "vue";
import { useRoute, RouterLink } from "vue-router";
import { Sun, Moon } from "@lucide/vue";
import { theme, toggleTheme } from "@/lib/theme";

const route = useRoute();

const navItems = [
  { label: "Status", code: "STS", to: "/status" },
  { label: "Traces", code: "TRC", to: "/traces" },
  { label: "Logs", code: "LOG", to: "/logs" },
  { label: "Metrics", code: "MTR", to: "/metrics" },
  { label: "Cluster", code: "K8S", to: "/cluster" },
  { label: "Config", code: "CFG", to: "/config" },
];

function isActive(to: string): boolean {
  const p = route.path;
  if (to === "/status") return p === "/" || p === "/status";
  return p.startsWith(to);
}

const today = computed(() => new Date().toISOString().slice(0, 10));
</script>

<template>
  <aside
    data-testid="sidebar"
    class="w-60 bg-surface-0 border-r-2 border-border flex flex-col h-full shrink-0 max-[960px]:hidden"
  >
    <!-- Header -->
    <div class="px-5 py-6 border-b-2 border-border flex justify-center">
      <RouterLink to="/status" class="flex flex-col items-center no-underline cursor-pointer">
        <span
          data-testid="sidebar-logo"
          class="font-display text-[38px] leading-none tracking-[0.18em] text-accent border-solid border-3 border-accent px-4 pt-2 pb-1.5 inline-block opacity-90"
          style="transform: rotate(-2.5deg); text-shadow: 2px 2px 0 rgba(0, 0, 0, 0.5)"
        >
          DEVRIG
        </span>
      </RouterLink>
    </div>

    <!-- Nav -->
    <nav class="flex-1 py-4 overflow-y-auto">
      <div class="px-3 mb-2">
        <div class="barcode" aria-hidden="true">
          <span v-for="n in 20" :key="n" />
        </div>
      </div>

      <RouterLink
        v-for="item in navItems"
        :key="item.to"
        :to="item.to"
        data-testid="sidebar-nav-item"
        :data-active="isActive(item.to) ? 'true' : 'false'"
        :class="[
          'flex items-center gap-3 px-5 py-3 no-underline transition-colors border-l-2',
          isActive(item.to)
            ? 'border-accent bg-accent/5 text-accent'
            : 'border-transparent text-text-secondary hover:text-accent hover:bg-accent/[0.03]',
        ]"
      >
        <span class="font-label text-[9px] tracking-[0.15em] text-text-muted w-8 shrink-0">{{
          item.code
        }}</span>
        <span class="font-display text-[22px] tracking-[0.1em] uppercase leading-none">{{
          item.label
        }}</span>
      </RouterLink>

      <div class="px-3 mt-4">
        <div class="barcode" aria-hidden="true">
          <span v-for="n in 16" :key="n" />
        </div>
      </div>
    </nav>

    <!-- Footer -->
    <div class="px-5 py-4 border-t-2 border-border flex items-center justify-between">
      <span class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em]">{{
        today
      }}</span>
      <button
        data-testid="theme-toggle"
        class="p-1.5 text-text-muted hover:text-accent rounded transition-colors border-none"
        :title="`Switch to ${theme === 'dark' ? 'light' : 'dark'} mode`"
        @click="toggleTheme"
      >
        <Sun v-if="theme === 'dark'" :size="14" />
        <Moon v-else :size="14" />
      </button>
    </div>
  </aside>
</template>
