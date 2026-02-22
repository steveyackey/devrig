import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { fetchStatus, type StatusResponse } from '../api';

const Overview: Component = () => {
  const [status, setStatus] = createSignal<StatusResponse | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [lastRefresh, setLastRefresh] = createSignal<string>('');

  const loadStatus = async () => {
    try {
      setError(null);
      const data = await fetchStatus();
      setStatus(data);
      setLastRefresh(new Date().toLocaleTimeString());
    } catch (err: any) {
      setError(err.message || 'Failed to load status');
    } finally {
      setLoading(false);
    }
  };

  // Initial load and auto-refresh every 5 seconds
  createEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 5000);
    onCleanup(() => clearInterval(interval));
  });

  const formatNumber = (n: number): string => n.toLocaleString();

  return (
    <div class="flex flex-col h-full">
      {/* Header */}
      <div class="px-6 py-4 border-b border-zinc-700/50 flex items-center justify-between">
        <div>
          <h2 class="text-lg font-semibold text-zinc-100">Overview</h2>
          <p class="text-sm text-zinc-500 mt-0.5">Telemetry pipeline overview</p>
        </div>
        <div class="flex items-center gap-3">
          <Show when={lastRefresh()}>
            <span class="text-xs text-zinc-600">Last refreshed: {lastRefresh()}</span>
          </Show>
          <button
            onClick={() => { setLoading(true); loadStatus(); }}
            class="bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm px-3 py-1.5 rounded-md border border-zinc-700"
          >
            Refresh
          </button>
        </div>
      </div>

      {/* Content */}
      <div class="flex-1 overflow-auto p-6">
        <Show when={error()}>
          <div class="mb-6 bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-center">
            <p class="text-red-400 text-sm">{error()}</p>
            <button
              onClick={() => { setLoading(true); loadStatus(); }}
              class="mt-2 text-blue-400 hover:text-blue-300 text-sm"
            >
              Retry
            </button>
          </div>
        </Show>

        <Show when={loading() && !status()}>
          <div class="py-12 text-center text-zinc-500 text-sm">
            Loading status...
          </div>
        </Show>

        <Show when={status()}>
          {(data) => (
            <div class="space-y-6">
              {/* Overview cards */}
              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                <StatCard
                  label="Traces"
                  value={formatNumber(data().trace_count)}
                  icon={'\u2261'}
                  color="blue"
                />
                <StatCard
                  label="Spans"
                  value={formatNumber(data().span_count)}
                  icon={'\u2500'}
                  color="cyan"
                />
                <StatCard
                  label="Logs"
                  value={formatNumber(data().log_count)}
                  icon={'\u25A4'}
                  color="green"
                />
                <StatCard
                  label="Metrics"
                  value={formatNumber(data().metric_count)}
                  icon={'\u25B3'}
                  color="purple"
                />
              </div>

              {/* Services */}
              <div class="bg-zinc-800/50 rounded-lg border border-zinc-700/30">
                <div class="px-5 py-4 border-b border-zinc-700/30">
                  <h3 class="text-sm font-semibold text-zinc-200">
                    Reporting Services ({data().services.length})
                  </h3>
                  <p class="text-xs text-zinc-500 mt-0.5">
                    Services that have sent telemetry data
                  </p>
                </div>

                <Show when={data().services.length === 0}>
                  <div class="px-5 py-8 text-center text-zinc-500 text-sm">
                    No services reporting yet.
                  </div>
                </Show>

                <Show when={data().services.length > 0}>
                  <div class="divide-y divide-zinc-700/30">
                    <For each={data().services}>
                      {(service) => (
                        <div class="px-5 py-3 flex items-center gap-3 hover:bg-zinc-800/40">
                          <span class="inline-block w-2 h-2 rounded-full bg-green-500" />
                          <span class="text-sm text-zinc-200 font-mono">{service}</span>
                          <div class="ml-auto flex gap-2">
                            <a
                              href={`#/traces`}
                              class="text-xs text-zinc-500 hover:text-blue-400"
                              onClick={() => {
                                // Navigation hint - traces view will need to filter
                              }}
                            >
                              View Traces
                            </a>
                            <a
                              href={`#/logs`}
                              class="text-xs text-zinc-500 hover:text-blue-400"
                            >
                              View Logs
                            </a>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>

              {/* Auto-refresh indicator */}
              <div class="text-center">
                <p class="text-xs text-zinc-600">
                  Auto-refreshes every 5 seconds
                </p>
              </div>
            </div>
          )}
        </Show>
      </div>
    </div>
  );
};

const StatCard: Component<{
  label: string;
  value: string;
  icon: string;
  color: string;
}> = (props) => {
  const colorClasses = (): { bg: string; text: string; border: string } => {
    switch (props.color) {
      case 'blue': return { bg: 'bg-blue-500/10', text: 'text-blue-400', border: 'border-blue-500/20' };
      case 'cyan': return { bg: 'bg-cyan-500/10', text: 'text-cyan-400', border: 'border-cyan-500/20' };
      case 'green': return { bg: 'bg-green-500/10', text: 'text-green-400', border: 'border-green-500/20' };
      case 'purple': return { bg: 'bg-purple-500/10', text: 'text-purple-400', border: 'border-purple-500/20' };
      default: return { bg: 'bg-zinc-500/10', text: 'text-zinc-400', border: 'border-zinc-500/20' };
    }
  };

  return (
    <div class={`rounded-lg border p-5 ${colorClasses().bg} ${colorClasses().border}`}>
      <div class="flex items-center justify-between mb-3">
        <span class="text-xs text-zinc-500 uppercase tracking-wider font-medium">{props.label}</span>
        <span class={`text-lg ${colorClasses().text}`}>{props.icon}</span>
      </div>
      <div class={`text-2xl font-semibold ${colorClasses().text} font-mono`}>{props.value}</div>
    </div>
  );
};

export default Overview;
