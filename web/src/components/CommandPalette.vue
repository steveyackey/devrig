<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { useRouter } from "vue-router";
import { Activity, ScrollText, BarChart3, CircleDot, Settings } from "@lucide/vue";

const router = useRouter();

const commands = [
  { label: "Status", icon: CircleDot, to: "/status" },
  { label: "Traces", icon: Activity, to: "/traces" },
  { label: "Logs", icon: ScrollText, to: "/logs" },
  { label: "Metrics", icon: BarChart3, to: "/metrics" },
  { label: "Config", icon: Settings, to: "/config" },
];

const open = ref(false);
const query = ref("");
const selectedIndex = ref(0);

const filtered = computed(() => {
  const q = query.value.toLowerCase().trim();
  if (!q) return commands;
  return commands.filter((c) => c.label.toLowerCase().includes(q));
});

function navigate(item: (typeof commands)[number]) {
  router.push(item.to);
  open.value = false;
}

function handleKeyDown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key === "k") {
    e.preventDefault();
    open.value = true;
    query.value = "";
    selectedIndex.value = 0;
    return;
  }
  if (!open.value) return;
  if (e.key === "Escape") {
    e.preventDefault();
    open.value = false;
  } else if (e.key === "ArrowDown") {
    e.preventDefault();
    selectedIndex.value = Math.min(selectedIndex.value + 1, filtered.value.length - 1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0);
  } else if (e.key === "Enter") {
    e.preventDefault();
    const item = filtered.value[selectedIndex.value];
    if (item) navigate(item);
  }
}

onMounted(() => document.addEventListener("keydown", handleKeyDown));
onUnmounted(() => document.removeEventListener("keydown", handleKeyDown));
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      data-testid="command-palette"
      class="fixed inset-0 z-50 flex items-start justify-center pt-[20vh]"
      @click="open = false"
    >
      <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" />
      <div
        class="relative w-full max-w-md bg-surface-1 border-2 border-border shadow-2xl animate-slide-up overflow-hidden"
        @click.stop
      >
        <div class="px-4 py-3 border-b-2 border-border">
          <input
            data-testid="command-palette-input"
            v-model="query"
            type="text"
            placeholder="Type a command..."
            class="w-full bg-transparent text-text-primary text-sm placeholder:text-text-muted focus:outline-none border-none"
            autofocus
            @input="selectedIndex = 0"
          />
        </div>

        <div class="py-2 max-h-64 overflow-auto">
          <div v-if="filtered.length === 0" class="px-4 py-6 text-center text-sm text-text-muted">
            No results found
          </div>
          <button
            v-for="(item, idx) in filtered"
            :key="item.to"
            data-testid="command-palette-item"
            class="w-full flex items-center gap-3 px-4 py-3 text-left transition-colors border-none cursor-pointer text-sm"
            :class="
              idx === selectedIndex
                ? 'bg-accent/10 text-accent'
                : 'text-text-secondary hover:bg-accent/5 hover:text-accent'
            "
            @click="navigate(item)"
            @mouseover="selectedIndex = idx"
          >
            <component :is="item.icon" :size="16" />
            <span class="font-display text-xl tracking-[0.1em] uppercase">{{ item.label }}</span>
          </button>
        </div>

        <div
          class="px-4 py-2 border-t-2 border-border flex items-center gap-3 text-[9px] text-text-muted font-label uppercase tracking-[0.1em]"
        >
          <span>↑↓ navigate</span>
          <span>↵ select</span>
          <span>esc close</span>
        </div>
      </div>
    </div>
  </Teleport>
</template>
