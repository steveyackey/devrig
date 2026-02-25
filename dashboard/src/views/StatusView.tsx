import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { ExternalLink } from 'lucide-solid';
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
      <div class="px-8 py-6 border-b-2 border-border flex items-start justify-between">
        <div>
          <h2
            class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
            style={{ "text-shadow": "2px 2px 0 rgba(0,0,0,0.5)" }}
          >
            System Status
          </h2>
          <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">
            Telemetry pipeline overview
          </p>
        </div>
        <div class="flex items-center gap-4">
          <Show when={lastRefresh()}>
            <span class="font-label text-[9px] text-text-secondary uppercase tracking-[0.08em]">
              Last refresh: {lastRefresh()}
            </span>
          </Show>
          <Button variant="default" size="sm" onClick={() => { setLoading(true); loadStatus(); }}>
            Refresh
          </Button>
        </div>
      </div>

      <div class="flex-1 overflow-auto p-7">
        <Show when={error()}>
          <div class="mb-6 border-2 border-error/30 bg-error/5 p-4 text-center">
            <p class="text-error text-sm">{error()}</p>
            <button onClick={() => { setLoading(true); loadStatus(); }} class="mt-2 text-accent hover:text-accent-hover text-sm">Retry</button>
          </div>
        </Show>

        <Show when={loading() && !status()}>
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
            <For each={[1, 2, 3, 4]}>{() => <Skeleton class="h-28" />}</For>
          </div>
          <Skeleton class="h-48" />
        </Show>

        <Show when={status()}>
          {(data) => (
            <div class="space-y-7 animate-fade-in">
              {/* Stat Cards */}
              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                <StatCard label="Traces" value={formatNumber(data().trace_count)} unit="distributed" />
                <StatCard label="Spans" value={formatNumber(data().span_count)} unit="total" />
                <StatCard label="Logs" value={formatNumber(data().log_count)} unit="records" />
                <StatCard label="Metrics" value={formatNumber(data().metric_count)} unit="points" />
              </div>

              {/* Services */}
              <div class="border-2 border-border bg-surface-1">
                <div class="px-6 py-4 border-b border-border flex items-center justify-between">
                  <h3
                    class="font-display text-[22px] text-accent tracking-[0.1em] uppercase"
                  >
                    Services ({serviceList().length || data().services.length})
                  </h3>
                  <span class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em]">
                    Configured services and their ports
                  </span>
                </div>

                <Show when={serviceList().length === 0 && data().services.length === 0}>
                  <div class="px-6 py-8 text-center text-text-secondary text-sm">
                    No services reporting yet.
                  </div>
                </Show>

                <Show when={serviceList().length > 0}>
                  <div>
                    <For each={serviceList()}>
                      {(svc) => {
                        const isReporting = () => data().services.includes(svc.name);
                        const isExited = () => svc.phase === 'stopped' || svc.phase === 'failed';
                        const isFailedExit = () => svc.phase === 'failed' || (isExited() && svc.exit_code != null && svc.exit_code !== 0);

                        const indicatorClass = () => {
                          if (isExited()) {
                            return isFailedExit() ? 'bg-error' : 'bg-surface-3';
                          }
                          return isReporting() ? 'bg-success animate-pulse-live' : 'bg-warning animate-pulse-live';
                        };

                        const indicatorShadow = () => {
                          if (isExited()) {
                            return isFailedExit() ? { "box-shadow": '0 0 6px rgba(239,68,68,0.4)' } : {};
                          }
                          return isReporting()
                            ? { "box-shadow": '0 0 6px rgba(74,222,128,0.3)' }
                            : { "box-shadow": '0 0 6px rgba(251,191,36,0.3)' };
                        };

                        return (
                          <div
                            data-testid="service-row"
                            class="px-6 py-3.5 flex items-center gap-3.5 border-b border-border last:border-b-0 hover:bg-accent/[0.03] transition-colors"
                          >
                            <span
                              data-testid="service-indicator"
                              class={`inline-block w-2 h-2 rounded-full border-solid ${indicatorClass()}`}
                              style={indicatorShadow()}
                            />
                            <span class="font-display text-lg text-text-primary tracking-[0.06em] uppercase">{svc.name}</span>
                            <Badge variant="default">{svc.kind}</Badge>
                            <Show when={isExited()}>
                              <Badge variant={isFailedExit() ? 'error' : 'default'}>
                                {isFailedExit() ? `exited (${svc.exit_code ?? '?'})` : 'exited (0)'}
                              </Badge>
                            </Show>
                            <Show when={svc.port}>
                              {(() => {
                                const proto = svc.protocol || 'http';
                                const isLinkable = proto === 'http' || proto === 'https';
                                return isLinkable ? (
                                  <a
                                    data-testid="service-port-link"
                                    href={`${proto}://localhost:${svc.port}`}
                                    target="_blank"
                                    rel="noopener"
                                    class="inline-flex items-center gap-1 text-xs font-mono text-text-muted hover:text-accent transition-colors"
                                  >
                                    :{svc.port}
                                    <ExternalLink size={10} />
                                  </a>
                                ) : (
                                  <span class="inline-flex items-center gap-1 text-xs font-mono text-text-muted">
                                    :{svc.port}
                                    <Badge variant="default">{proto}</Badge>
                                  </span>
                                );
                              })()}
                              <Show when={svc.port_auto}>
                                <span class="text-[10px] text-text-muted">(auto)</span>
                              </Show>
                            </Show>
                            <div class="ml-auto flex gap-2.5">
                              <a href="#/traces" class="font-label text-[9px] text-text-muted hover:text-accent uppercase tracking-[0.08em] transition-colors">
                                Traces
                              </a>
                              <a href="#/logs" class="font-label text-[9px] text-text-muted hover:text-accent uppercase tracking-[0.08em] transition-colors">
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
                  <div>
                    <For each={data().services}>
                      {(service) => (
                        <div
                          data-testid="service-row"
                          class="px-6 py-3.5 flex items-center gap-3.5 border-b border-border last:border-b-0 hover:bg-accent/[0.03] transition-colors"
                        >
                          <span
                            data-testid="service-indicator"
                            class="inline-block w-2 h-2 rounded-full border-solid bg-success animate-pulse-live"
                            style={{ "box-shadow": '0 0 6px rgba(74,222,128,0.3)' }}
                          />
                          <span class="font-display text-lg text-text-primary tracking-[0.06em] uppercase">{service}</span>
                          <div class="ml-auto flex gap-2.5">
                            <a href="#/traces" class="font-label text-[9px] text-text-muted hover:text-accent uppercase tracking-[0.08em] transition-colors">
                              Traces
                            </a>
                            <a href="#/logs" class="font-label text-[9px] text-text-muted hover:text-accent uppercase tracking-[0.08em] transition-colors">
                              Logs
                            </a>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>

              <div class="text-center">
                <p class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em]">
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
  unit: string;
}> = (props) => {
  return (
    <div data-testid="stat-card" class="bg-surface-1 p-6 border-2 border-border relative hover:border-border-hover transition-colors">
      <div class="absolute top-2.5 right-3 flex items-center gap-1 font-label text-[9px] text-success tracking-[0.06em]">
        <span class="text-[8px]" aria-hidden="true">&#9650;</span> OK
      </div>
      <div data-testid="stat-card-label" class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em] mb-1.5">
        {props.label}
      </div>
      <div
        data-testid="stat-card-value"
        class="font-display text-[56px] leading-none text-accent"
        style={{ "text-shadow": "1px 1px 0 rgba(0,0,0,0.5)" }}
      >
        {props.value}
      </div>
      <div class="font-label text-[9px] text-text-secondary mt-1">
        {props.unit}
      </div>
    </div>
  );
};

export default StatusView;
