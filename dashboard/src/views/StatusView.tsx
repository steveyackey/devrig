import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { Activity, ScrollText, BarChart3, Minus, ExternalLink } from 'lucide-solid';
import { fetchStatus, fetchServices, type StatusResponse, type ServiceInfo } from '../api';
import { Badge, Card, Skeleton, Button } from '../components/ui';

const StatusView: Component = () => {
  const [status, setStatus] = createSignal<StatusResponse | null>(null);
  const [serviceList, setServiceList] = createSignal<ServiceInfo[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [lastRefresh, setLastRefresh] = createSignal<string>('');

  const loadStatus = async () => {
    try {
      setError(null);
      const [data, services] = await Promise.all([fetchStatus(), fetchServices()]);
      setStatus(data);
      setServiceList(services);
      setLastRefresh(new Date().toLocaleTimeString());
    } catch (err: any) {
      setError(err.message || 'Failed to load status');
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 5000);
    onCleanup(() => clearInterval(interval));
  });

  const formatNumber = (n: number): string => n.toLocaleString();

  return (
    <div data-testid="status-view" class="flex flex-col h-full">
      <div class="px-6 py-5 border-b border-border flex items-center justify-between">
        <div>
          <h2 class="text-lg font-semibold text-text-primary">System Status</h2>
          <p class="text-sm text-text-muted mt-0.5">Telemetry pipeline overview</p>
        </div>
        <div class="flex items-center gap-3">
          <Show when={lastRefresh()}>
            <span class="text-xs text-text-muted">Last refreshed: {lastRefresh()}</span>
          </Show>
          <Button variant="outline" size="sm" onClick={() => { setLoading(true); loadStatus(); }}>
            Refresh
          </Button>
        </div>
      </div>

      <div class="flex-1 overflow-auto p-6">
        <Show when={error()}>
          <div class="mb-6 bg-error/10 border border-error/20 rounded-lg p-4 text-center">
            <p class="text-error text-sm">{error()}</p>
            <button onClick={() => { setLoading(true); loadStatus(); }} class="mt-2 text-accent hover:text-accent-hover text-sm">Retry</button>
          </div>
        </Show>

        <Show when={loading() && !status()}>
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
            <For each={[1, 2, 3, 4]}>{() => <Skeleton class="h-24 rounded-lg" />}</For>
          </div>
          <Skeleton class="h-48 rounded-lg" />
        </Show>

        <Show when={status()}>
          {(data) => (
            <div class="space-y-6 animate-fade-in">
              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-5">
                <StatCard
                  label="Traces"
                  value={formatNumber(data().trace_count)}
                  icon={Activity}
                  gradient="from-accent/10 to-accent/5"
                  iconColor="text-accent"
                  valueColor="text-accent"
                  borderColor="border-accent/20"
                />
                <StatCard
                  label="Spans"
                  value={formatNumber(data().span_count)}
                  icon={Minus}
                  gradient="from-info/10 to-info/5"
                  iconColor="text-info"
                  valueColor="text-info"
                  borderColor="border-info/20"
                />
                <StatCard
                  label="Logs"
                  value={formatNumber(data().log_count)}
                  icon={ScrollText}
                  gradient="from-success/10 to-success/5"
                  iconColor="text-success"
                  valueColor="text-success"
                  borderColor="border-success/20"
                />
                <StatCard
                  label="Metrics"
                  value={formatNumber(data().metric_count)}
                  icon={BarChart3}
                  gradient="from-[#a855f7]/10 to-[#a855f7]/5"
                  iconColor="text-[#a855f7]"
                  valueColor="text-[#a855f7]"
                  borderColor="border-[#a855f7]/20"
                />
              </div>

              <Card>
                <div class="px-5 py-4 border-b border-border">
                  <h3 class="text-sm font-semibold text-text-primary">
                    Services ({serviceList().length || data().services.length})
                  </h3>
                  <p class="text-xs text-text-muted mt-0.5">
                    Configured services and their ports
                  </p>
                </div>

                <Show when={serviceList().length === 0 && data().services.length === 0}>
                  <div class="px-5 py-8 text-center text-text-muted text-sm">
                    No services reporting yet.
                  </div>
                </Show>

                <Show when={serviceList().length > 0}>
                  <div class="divide-y divide-border">
                    <For each={serviceList()}>
                      {(svc) => {
                        const isReporting = () => data().services.includes(svc.name);
                        return (
                          <div data-testid="service-row" class="px-5 py-4 flex items-center gap-3 hover:bg-surface-2/40 transition-colors">
                            <span
                              data-testid="service-indicator"
                              class={`inline-block w-2 h-2 rounded-full ${isReporting() ? 'bg-success animate-pulse-live' : 'bg-surface-3'}`}
                            />
                            <span class="text-sm text-text-primary font-mono">{svc.name}</span>
                            <Badge variant="default" class="text-[10px] px-1.5 py-0">{svc.kind}</Badge>
                            <Show when={svc.port}>
                              <a
                                data-testid="service-port-link"
                                href={`http://localhost:${svc.port}`}
                                target="_blank"
                                rel="noopener"
                                class="inline-flex items-center gap-1 text-xs font-mono text-accent hover:text-accent-hover transition-colors"
                              >
                                :{svc.port}
                                <ExternalLink size={10} />
                              </a>
                              <Show when={svc.port_auto}>
                                <span class="text-[10px] text-text-muted">(auto)</span>
                              </Show>
                            </Show>
                            <div class="ml-auto flex gap-2">
                              <a href="#/traces" class="text-xs text-text-muted hover:text-accent transition-colors">
                                Traces
                              </a>
                              <a href="#/logs" class="text-xs text-text-muted hover:text-accent transition-colors">
                                Logs
                              </a>
                            </div>
                          </div>
                        );
                      }}
                    </For>
                  </div>
                </Show>

                <Show when={serviceList().length === 0 && data().services.length > 0}>
                  <div class="divide-y divide-border">
                    <For each={data().services}>
                      {(service) => (
                        <div data-testid="service-row" class="px-5 py-4 flex items-center gap-3 hover:bg-surface-2/40 transition-colors">
                          <span data-testid="service-indicator" class="inline-block w-2 h-2 rounded-full bg-success animate-pulse-live" />
                          <span class="text-sm text-text-primary font-mono">{service}</span>
                          <div class="ml-auto flex gap-2">
                            <a href="#/traces" class="text-xs text-text-muted hover:text-accent transition-colors">
                              Traces
                            </a>
                            <a href="#/logs" class="text-xs text-text-muted hover:text-accent transition-colors">
                              Logs
                            </a>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </Card>

              <div class="text-center">
                <p class="text-xs text-text-muted">
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
  icon: Component<{ size?: number; class?: string }>;
  gradient: string;
  iconColor: string;
  valueColor: string;
  borderColor: string;
}> = (props) => {
  const Icon = props.icon;
  return (
    <div data-testid="stat-card" class={`rounded-lg border p-6 bg-gradient-to-br ${props.gradient} ${props.borderColor}`}>
      <div class="flex items-center justify-between mb-3">
        <span data-testid="stat-card-label" class="text-xs text-text-muted uppercase tracking-wider font-medium">{props.label}</span>
        <Icon size={20} class={props.iconColor} />
      </div>
      <div data-testid="stat-card-value" class={`text-2xl font-semibold ${props.valueColor} font-mono`}>{props.value}</div>
    </div>
  );
};

export default StatusView;
