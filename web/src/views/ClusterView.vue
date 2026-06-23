<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { fetchCluster, type ClusterResponse } from "@/api";

const cluster = ref<ClusterResponse | null>(null);
const loading = ref(true);
const error = ref<string | null>(null);

// The API returns deployed_services / installed_addons as maps keyed by name
// (and they can be null when empty). Normalize to arrays for the table.
const services = computed(() =>
  Object.entries(cluster.value?.deployed_services ?? {}).map(([name, s]) => ({ name, ...s })),
);
const addons = computed(() =>
  Object.entries(cluster.value?.installed_addons ?? {}).map(([name, a]) => ({ name, ...a })),
);
const pods = computed(() => cluster.value?.pods ?? []);

onMounted(async () => {
  try {
    cluster.value = await fetchCluster();
  } catch (err: unknown) {
    error.value = err instanceof Error ? err.message : "Failed to load cluster";
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <div class="flex flex-col h-full">
    <div class="px-8 py-6 border-b-2 border-border">
      <h2
        class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
        style="text-shadow: 2px 2px 0 rgba(0, 0, 0, 0.5)"
      >
        Cluster
      </h2>
      <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">
        k3d cluster status
      </p>
    </div>

    <div class="flex-1 overflow-auto p-7">
      <div v-if="loading" class="space-y-4">
        <div class="h-28 bg-surface-2 animate-skeleton" />
      </div>
      <div v-else-if="error" class="text-center text-error text-sm">{{ error }}</div>
      <div v-else-if="!cluster" class="text-center text-text-muted text-sm py-16">
        <p class="font-display text-2xl text-accent/40 mb-2">No Cluster</p>
        <p>
          Add a <code class="font-mono text-accent/60">[cluster]</code> block to your devrig config
          to enable k3d cluster management.
        </p>
      </div>
      <div v-else class="space-y-6 animate-fade-in">
        <!-- Cluster info -->
        <div class="border-2 border-border bg-surface-1 p-6">
          <div class="grid grid-cols-2 gap-4 text-sm">
            <div>
              <div class="font-label text-[9px] text-text-muted uppercase tracking-[0.1em] mb-1">
                Cluster Name
              </div>
              <div class="font-mono text-text-primary">{{ cluster.cluster_name }}</div>
            </div>
            <div>
              <div class="font-label text-[9px] text-text-muted uppercase tracking-[0.1em] mb-1">
                Kubeconfig
              </div>
              <div class="font-mono text-text-muted truncate">{{ cluster.kubeconfig_path }}</div>
            </div>
            <div v-if="cluster.registry_name">
              <div class="font-label text-[9px] text-text-muted uppercase tracking-[0.1em] mb-1">
                Registry
              </div>
              <div class="font-mono text-text-primary">
                {{ cluster.registry_name }}:{{ cluster.registry_port }}
              </div>
            </div>
          </div>
        </div>

        <!-- Deployed services -->
        <div v-if="services.length > 0" class="border-2 border-border bg-surface-1">
          <div class="px-6 py-3 border-b border-border">
            <h3 class="font-display text-[18px] text-accent tracking-[0.1em] uppercase">
              Deployed Services
            </h3>
          </div>
          <table class="w-full text-xs">
            <thead>
              <tr
                class="border-b border-border text-[9px] font-label text-text-muted uppercase tracking-[0.1em]"
              >
                <th class="px-6 py-2 text-left font-normal">Name</th>
                <th class="px-4 py-2 text-left font-normal">Image Tag</th>
                <th class="px-4 py-2 text-left font-normal">Last Deployed</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="svc in services" :key="svc.name" class="border-b border-border">
                <td
                  class="px-6 py-2.5 font-display text-lg text-text-primary tracking-[0.06em] uppercase"
                >
                  {{ svc.name }}
                </td>
                <td class="px-4 py-2.5 font-mono text-text-muted">{{ svc.image_tag }}</td>
                <td class="px-4 py-2.5 text-text-muted">{{ svc.last_deployed }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <!-- Addons -->
        <div v-if="addons.length > 0" class="border-2 border-border bg-surface-1">
          <div class="px-6 py-3 border-b border-border">
            <h3 class="font-display text-[18px] text-accent tracking-[0.1em] uppercase">Addons</h3>
          </div>
          <table class="w-full text-xs">
            <thead>
              <tr
                class="border-b border-border text-[9px] font-label text-text-muted uppercase tracking-[0.1em]"
              >
                <th class="px-6 py-2 text-left font-normal">Name</th>
                <th class="px-4 py-2 text-left font-normal">Type</th>
                <th class="px-4 py-2 text-left font-normal">Namespace</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="addon in addons" :key="addon.name" class="border-b border-border">
                <td
                  class="px-6 py-2.5 font-display text-lg text-text-primary tracking-[0.06em] uppercase"
                >
                  {{ addon.name }}
                </td>
                <td class="px-4 py-2.5 text-text-muted">{{ addon.addon_type }}</td>
                <td class="px-4 py-2.5 font-mono text-text-muted">{{ addon.namespace }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <!-- Pods -->
        <div v-if="pods.length > 0" class="border-2 border-border bg-surface-1">
          <div class="px-6 py-3 border-b border-border">
            <h3 class="font-display text-[18px] text-accent tracking-[0.1em] uppercase">Pods</h3>
          </div>
          <table class="w-full text-xs">
            <thead>
              <tr
                class="border-b border-border text-[9px] font-label text-text-muted uppercase tracking-[0.1em]"
              >
                <th class="px-6 py-2 text-left font-normal">Name</th>
                <th class="px-4 py-2 text-left font-normal">Namespace</th>
                <th class="px-4 py-2 text-left font-normal">Status</th>
                <th class="px-4 py-2 text-left font-normal">Ready</th>
                <th class="px-4 py-2 text-left font-normal">Restarts</th>
                <th class="px-4 py-2 text-left font-normal">Age</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="pod in pods" :key="`${pod.namespace}/${pod.name}`" class="border-b border-border">
                <td class="px-6 py-2.5 font-mono text-sm text-text-primary">
                  {{ pod.name }}
                </td>
                <td class="px-4 py-2.5 font-mono text-text-muted">{{ pod.namespace }}</td>
                <td class="px-4 py-2.5 text-text-muted">
                  <span
                    :class="{
                      'text-success': pod.phase === 'Running',
                      'text-warning': pod.phase === 'Pending',
                      'text-error': pod.phase === 'Failed' || pod.phase === 'Unknown',
                    }"
                  >
                    {{ pod.phase }}
                  </span>
                </td>
                <td class="px-4 py-2.5 font-mono text-text-muted">{{ pod.ready }}</td>
                <td class="px-4 py-2.5 text-text-muted">{{ pod.restarts }}</td>
                <td class="px-4 py-2.5 text-text-muted">{{ pod.age }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>
  </div>
</template>
