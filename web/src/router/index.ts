import { createRouter, createWebHashHistory } from "vue-router";
import StatusView from "@/views/StatusView.vue";
import TracesView from "@/views/TracesView.vue";
import TraceDetailView from "@/views/TraceDetailView.vue";
import LogsView from "@/views/LogsView.vue";
import MetricsView from "@/views/MetricsView.vue";
import ClusterView from "@/views/ClusterView.vue";
import ConfigView from "@/views/ConfigView.vue";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/status" },
    { path: "/status", component: StatusView },
    { path: "/traces", component: TracesView },
    { path: "/traces/:id", component: TraceDetailView },
    { path: "/logs", component: LogsView },
    { path: "/metrics", component: MetricsView },
    { path: "/cluster", component: ClusterView },
    { path: "/config", component: ConfigView },
  ],
});

export default router;
