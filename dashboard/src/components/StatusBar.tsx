import { Component, createSignal, createEffect, onCleanup } from 'solid-js';
import { fetchStatus, type StatusResponse } from '../api';
import { Badge } from './ui';

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
    <footer data-testid="status-bar" class="h-9 bg-surface-1 border-t border-border flex items-center px-4 text-xs text-text-muted gap-6 shrink-0">
      <div class="flex items-center gap-1.5">
        <span
          data-testid="status-bar-ws-indicator"
          class={`inline-block w-2 h-2 rounded-full ${
            wsConnected() ? 'bg-success animate-pulse-live' : 'bg-surface-3'
          }`}
        />
        <span data-testid="status-bar-ws-status">{wsConnected() ? 'Live' : 'Disconnected'}</span>
      </div>

      {status() && (
        <>
          <div class="flex items-center gap-1.5">
            <span class="text-text-muted">Traces:</span>
            <Badge variant="default" class="text-[10px] px-1.5 py-0">
              <span data-testid="status-bar-traces-count">{status()!.trace_count.toLocaleString()}</span>
            </Badge>
          </div>
          <div class="flex items-center gap-1.5">
            <span class="text-text-muted">Spans:</span>
            <Badge variant="default" class="text-[10px] px-1.5 py-0">
              <span data-testid="status-bar-spans-count">{status()!.span_count.toLocaleString()}</span>
            </Badge>
          </div>
          <div class="flex items-center gap-1.5">
            <span class="text-text-muted">Logs:</span>
            <Badge variant="default" class="text-[10px] px-1.5 py-0">
              <span data-testid="status-bar-logs-count">{status()!.log_count.toLocaleString()}</span>
            </Badge>
          </div>
          <div class="flex items-center gap-1.5">
            <span class="text-text-muted">Metrics:</span>
            <Badge variant="default" class="text-[10px] px-1.5 py-0">
              <span data-testid="status-bar-metrics-count">{status()!.metric_count.toLocaleString()}</span>
            </Badge>
          </div>
          <div class="flex items-center gap-1.5">
            <span class="text-text-muted">Services:</span>
            <Badge variant="default" class="text-[10px] px-1.5 py-0">
              <span data-testid="status-bar-services-count">{status()!.services.length}</span>
            </Badge>
          </div>
        </>
      )}

      <div class="ml-auto text-text-muted">
        {lastUpdated() && `Updated ${lastUpdated()}`}
      </div>
    </footer>
  );
};

export default StatusBar;
