import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { fetchCluster, fetchServices, type ClusterResponse, type ServiceInfo } from '../api';
import { Badge, Card, Skeleton, Button } from '../components/ui';

const ClusterView: Component = () => {
  const [cluster, setCluster] = createSignal<ClusterResponse | null>(null);
  const [services, setServices] = createSignal<ServiceInfo[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [lastRefresh, setLastRefresh] = createSignal<string>('');

  const loadData = async () => {
    try {
      setError(null);
      const [clusterData, svcData] = await Promise.all([fetchCluster(), fetchServices()]);
      setCluster(clusterData);
      setServices(svcData);
      setLastRefresh(new Date().toLocaleTimeString());
    } catch (err: any) {
      setError(err.message || 'Failed to load cluster status');
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    loadData();
    const interval = setInterval(loadData, 5000);
    onCleanup(() => clearInterval(interval));
  });

  const formatTime = (iso: string): string => {
    try {
      const d = new Date(iso);
      return d.toLocaleString();
    } catch {
      return iso;
    }
  };

  const formatRelativeTime = (iso: string): string => {
    try {
      const d = new Date(iso);
      const now = Date.now();
      const diff = now - d.getTime();
      const mins = Math.floor(diff / 60000);
      if (mins < 1) return 'just now';
      if (mins < 60) return `${mins}m ago`;
      const hours = Math.floor(mins / 60);
      if (hours < 24) return `${hours}h ago`;
      const days = Math.floor(hours / 24);
      return `${days}d ago`;
    } catch {
      return iso;
    }
  };

  const clusterServices = () => services().filter(s => s.kind === 'docker' || s.kind === 'service' || s.kind === 'compose');

  return (
    <div data-testid="cluster-view" class="flex flex-col h-full">
      <div class="px-8 py-6 border-b-2 border-border flex items-start justify-between">
        <div>
          <h2
            class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
            style={{ "text-shadow": "2px 2px 0 rgba(0,0,0,0.5)" }}
          >
            Cluster
          </h2>
          <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">
            Kubernetes cluster, images & addons
          </p>
        </div>
        <div class="flex items-center gap-4">
          <Show when={lastRefresh()}>
            <span class="font-label text-[9px] text-text-secondary uppercase tracking-[0.08em]">
              Last refresh: {lastRefresh()}
            </span>
          </Show>
          <Button variant="default" size="sm" onClick={() => { setLoading(true); loadData(); }}>
            Refresh
          </Button>
        </div>
      </div>

      <div class="flex-1 overflow-auto p-7">
        <Show when={error()}>
          <div class="mb-6 border-2 border-error/30 bg-error/5 p-4 text-center">
            <p class="text-error text-sm">{error()}</p>
            <button onClick={() => { setLoading(true); loadData(); }} class="mt-2 text-accent hover:text-accent-hover text-sm">Retry</button>
          </div>
        </Show>

        <Show when={loading() && !cluster()}>
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 mb-6">
            <For each={[1, 2, 3]}>{() => <Skeleton class="h-28" />}</For>
          </div>
          <Skeleton class="h-48" />
        </Show>

        {/* No cluster configured */}
        <Show when={!loading() && !cluster()}>
          <div class="flex flex-col items-center justify-center py-20">
            <div class="font-display text-6xl text-accent/20 mb-4">K8S</div>
            <p class="text-text-secondary text-sm">No cluster configured in this project.</p>
            <p class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em] mt-2">
              Add a [cluster] section to devrig.toml to enable Kubernetes
            </p>
          </div>
        </Show>

        <Show when={cluster()}>
          {(data) => (
            <div class="space-y-7 animate-fade-in">
              {/* Stat Cards */}
              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                <ClusterStatCard
                  label="Cluster"
                  value={data().cluster_name}
                  detail="k3d"
                />
                <ClusterStatCard
                  label="Registry"
                  value={data().registry ? `${data().registry!.name}` : 'none'}
                  detail={data().registry ? `:${data().registry!.port}` : 'disabled'}
                />
                <ClusterStatCard
                  label="Images"
                  value={String(data().deployed_services.length)}
                  detail="deployed"
                />
              </div>

              {/* All Services — unified view of docker + services + compose */}
              <div class="border-2 border-border bg-surface-1">
                <div class="px-6 py-4 border-b border-border flex items-center justify-between">
                  <h3 class="font-display text-[22px] text-accent tracking-[0.1em] uppercase">
                    Services ({clusterServices().length})
                  </h3>
                  <span class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em]">
                    All running services
                  </span>
                </div>
                <Show when={clusterServices().length === 0}>
                  <div class="px-6 py-8 text-center text-text-secondary text-sm">
                    No services running.
                  </div>
                </Show>
                <Show when={clusterServices().length > 0}>
                  <div>
                    <For each={clusterServices()}>
                      {(svc) => {
                        const isExited = () => svc.phase === 'stopped' || svc.phase === 'failed';
                        const isFailedExit = () => svc.phase === 'failed' || (isExited() && svc.exit_code != null && svc.exit_code !== 0);

                        const indicatorClass = () => {
                          if (isExited()) return isFailedExit() ? 'bg-error' : 'bg-surface-3';
                          return 'bg-success animate-pulse-live';
                        };

                        const indicatorShadow = () => {
                          if (isExited()) return isFailedExit() ? { "box-shadow": '0 0 6px rgba(239,68,68,0.4)' } : {};
                          return { "box-shadow": '0 0 6px rgba(74,222,128,0.3)' };
                        };

                        const proto = () => svc.protocol || 'http';
                        const isLinkable = () => proto() === 'http' || proto() === 'https';

                        return (
                          <div class="px-6 py-3.5 flex items-center gap-3.5 border-b border-border last:border-b-0 hover:bg-accent/[0.03] transition-colors">
                            <span
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
                              {isLinkable() ? (
                                <a
                                  href={`${proto()}://localhost:${svc.port}`}
                                  target="_blank"
                                  rel="noopener"
                                  class="text-xs font-mono text-text-muted hover:text-accent transition-colors"
                                >
                                  :{svc.port}
                                </a>
                              ) : (
                                <span class="text-xs font-mono text-text-muted">
                                  :{svc.port} <Badge variant="default">{proto()}</Badge>
                                </span>
                              )}
                              <Show when={svc.port_auto}>
                                <span class="text-[10px] text-text-muted">(auto)</span>
                              </Show>
                            </Show>
                          </div>
                        );
                      }}
                    </For>
                  </div>
                </Show>
              </div>

              {/* Deployed Images */}
              <div class="border-2 border-border bg-surface-1">
                <div class="px-6 py-4 border-b border-border flex items-center justify-between">
                  <h3 class="font-display text-[22px] text-accent tracking-[0.1em] uppercase">
                    Deployed Images ({data().deployed_services.length})
                  </h3>
                  <span class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em]">
                    Built & pushed to cluster registry
                  </span>
                </div>
                <Show when={data().deployed_services.length === 0}>
                  <div class="px-6 py-8 text-center text-text-secondary text-sm">
                    No images deployed to the cluster yet.
                  </div>
                </Show>
                <Show when={data().deployed_services.length > 0}>
                  <div>
                    <For each={data().deployed_services}>
                      {(deploy) => (
                        <div class="px-6 py-3.5 flex items-center gap-3.5 border-b border-border last:border-b-0 hover:bg-accent/[0.03] transition-colors">
                          <span
                            class="inline-block w-2 h-2 rounded-full border-solid bg-accent"
                            style={{ "box-shadow": '0 0 6px rgba(255,214,0,0.3)' }}
                          />
                          <span class="font-display text-lg text-text-primary tracking-[0.06em] uppercase">{deploy.name}</span>
                          <span class="font-mono text-xs text-text-muted truncate max-w-[200px]" title={deploy.image_tag}>
                            {deploy.image_tag.length > 12 ? deploy.image_tag.slice(0, 12) + '...' : deploy.image_tag}
                          </span>
                          <div class="ml-auto flex items-center gap-3">
                            <span class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em]" title={formatTime(deploy.last_deployed)}>
                              {formatRelativeTime(deploy.last_deployed)}
                            </span>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>

              {/* Addons */}
              <div class="border-2 border-border bg-surface-1">
                <div class="px-6 py-4 border-b border-border flex items-center justify-between">
                  <h3 class="font-display text-[22px] text-accent tracking-[0.1em] uppercase">
                    Addons ({data().addons.length})
                  </h3>
                  <span class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em]">
                    Helm charts, manifests & kustomize
                  </span>
                </div>
                <Show when={data().addons.length === 0}>
                  <div class="px-6 py-8 text-center text-text-secondary text-sm">
                    No addons installed.
                  </div>
                </Show>
                <Show when={data().addons.length > 0}>
                  <div>
                    <For each={data().addons}>
                      {(addon) => (
                        <div class="px-6 py-3.5 flex items-center gap-3.5 border-b border-border last:border-b-0 hover:bg-accent/[0.03] transition-colors">
                          <span
                            class="inline-block w-2 h-2 rounded-full border-solid bg-success"
                            style={{ "box-shadow": '0 0 6px rgba(74,222,128,0.3)' }}
                          />
                          <span class="font-display text-lg text-text-primary tracking-[0.06em] uppercase">{addon.name}</span>
                          <Badge variant="default">{addon.addon_type}</Badge>
                          <span class="font-mono text-xs text-text-muted">{addon.namespace}</span>
                          <div class="ml-auto flex items-center gap-3">
                            <span class="font-label text-[9px] text-text-muted uppercase tracking-[0.08em]" title={formatTime(addon.installed_at)}>
                              {formatRelativeTime(addon.installed_at)}
                            </span>
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

const ClusterStatCard: Component<{
  label: string;
  value: string;
  detail: string;
}> = (props) => {
  return (
    <div class="bg-surface-1 p-6 border-2 border-border relative hover:border-border-hover transition-colors">
      <div class="absolute top-2.5 right-3 flex items-center gap-1 font-label text-[9px] text-success tracking-[0.06em]">
        <span class="text-[8px]" aria-hidden="true">&#9650;</span> OK
      </div>
      <div class="font-label text-[10px] text-text-muted uppercase tracking-[0.15em] mb-1.5">
        {props.label}
      </div>
      <div
        class="font-display text-[36px] leading-none text-accent truncate"
        style={{ "text-shadow": "1px 1px 0 rgba(0,0,0,0.5)" }}
        title={props.value}
      >
        {props.value}
      </div>
      <div class="font-label text-[9px] text-text-secondary mt-1">
        {props.detail}
      </div>
    </div>
  );
};

export default ClusterView;
