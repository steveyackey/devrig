import { Component, createSignal, createEffect, onCleanup } from 'solid-js';
import { fetchStatus, type StatusResponse } from '../api';

const StatusBar: Component = () => {
  const [status, setStatus] = createSignal<StatusResponse | null>(null);
  const [wsConnected, setWsConnected] = createSignal(false);
  const [lastUpdated, setLastUpdated] = createSignal<string>('');

  const loadStatus = async () => {
    try {
      const data = await fetchStatus();
      setStatus(data);
      setLastUpdated(new Date().toLocaleTimeString());
    } catch {
      // silently handle - status bar is non-critical
    }
  };

  createEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 10000);
    onCleanup(() => clearInterval(interval));
  });

  // Monitor WebSocket connectivity
  createEffect(() => {
    const checkWs = () => {
      setWsConnected(!!(window as any).__devrig_ws_connected);
    };
    checkWs();
    const interval = setInterval(checkWs, 2000);
    onCleanup(() => clearInterval(interval));
  });

  return (
    <footer data-testid="status-bar" class="h-10 bg-surface-0 border-t-2 border-border flex items-center px-6 text-[9px] font-label text-text-secondary gap-4 shrink-0">
      <span class="flex items-center gap-1.5">
        <span
          data-testid="status-bar-ws-indicator"
          class={`inline-block w-1.5 h-1.5 rounded-full border-solid ${
            wsConnected() ? 'bg-success animate-pulse-live' : 'bg-surface-3'
          }`}
          style={wsConnected() ? { "box-shadow": '0 0 4px rgba(74,222,128,0.4)' } : {}}
        />
        <span data-testid="status-bar-ws-status" class={wsConnected() ? 'text-success' : 'text-text-muted'}>
          {wsConnected() ? 'Live' : 'Disconnected'}
        </span>
      </span>

      {status() && (
        <>
          <span class="text-accent/10" aria-hidden="true">&middot;</span>
          <span>Traces: <span data-testid="status-bar-traces-count">{status()!.trace_count.toLocaleString()}</span></span>
          <span class="text-accent/10" aria-hidden="true">&middot;</span>
          <span>Spans: <span data-testid="status-bar-spans-count">{status()!.span_count.toLocaleString()}</span></span>
          <span class="text-accent/10" aria-hidden="true">&middot;</span>
          <span>Logs: <span data-testid="status-bar-logs-count">{status()!.log_count.toLocaleString()}</span></span>
          <span class="text-accent/10" aria-hidden="true">&middot;</span>
          <span>Metrics: <span data-testid="status-bar-metrics-count">{status()!.metric_count.toLocaleString()}</span></span>
          <span class="text-accent/10" aria-hidden="true">&middot;</span>
          <span>Services: <span data-testid="status-bar-services-count">{status()!.services.length}</span></span>
        </>
      )}

      <span class="ml-auto text-text-muted">
        {lastUpdated()}
      </span>
    </footer>
  );
};

export default StatusBar;
